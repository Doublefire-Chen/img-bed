//! The Image store: ingesting uploads and looking up Images and their Paths.
//!
//! Files live under `<data_dir>/images/<aa>/<bb>/<sha256>` (sharded by the
//! first two bytes of the hash) and are written byte-for-byte as uploaded,
//! atomically: to a temporary file in `<data_dir>/tmp`, synced, then renamed
//! into place. The database only refers to them by hash.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::error::ApiError;
use crate::image_type::ImageType;
use crate::path_template::{Template, UploadFacts};
use crate::state::AppState;

/// Leading parts of the address space that can never be a Path, because the
/// admin area and the API live there. `admin` alone is reserved too: it
/// redirects to `admin/`.
pub const RESERVED_PREFIXES: [&str; 2] = ["admin", "api"];

/// How often a Path Template is rendered again after a collision or a
/// Reserved Prefix hit before the upload fails. Only a template with almost
/// no randomness left (e.g. `{random:1}` after 62 uploads) gets near this.
const MAX_PATH_DRAWS: usize = 1000;

/// Whether `path` (without a leading `/`) falls under a Reserved Prefix.
pub fn is_reserved(path: &str) -> bool {
    RESERVED_PREFIXES.iter().any(|prefix| {
        path.strip_prefix(prefix)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

/// Where the file with this hash is stored.
pub fn file_path(data_dir: &Path, sha256: &str) -> PathBuf {
    data_dir
        .join("images")
        .join(&sha256[0..2])
        .join(&sha256[2..4])
        .join(sha256)
}

/// An Image record with its Paths (Primary first, then oldest first).
#[derive(Debug, Clone)]
pub struct Image {
    pub id: i64,
    pub sha256: String,
    pub size: i64,
    pub mime: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub original_filename: String,
    /// Unix seconds.
    pub uploaded_at: i64,
    pub paths: Vec<ImagePath>,
}

#[derive(Debug, Clone)]
pub struct ImagePath {
    pub path: String,
    pub is_primary: bool,
}

/// The outcome of a successful ingest.
#[derive(Debug)]
pub struct Ingested {
    pub image: Image,
    /// The bytes matched an existing Image; nothing new was created.
    pub duplicate: bool,
}

/// Stores one uploaded file: checks its size and type, writes it to disk and
/// records the Image with its first Path from the Path Template.
///
/// Identical bytes are always the same Image: uploading them again returns
/// the existing Image with `duplicate: true`.
pub async fn ingest(
    state: &AppState,
    original_filename: &str,
    bytes: Vec<u8>,
) -> Result<Ingested, ApiError> {
    let limit = state.config.upload_limit_bytes;
    if bytes.len() as u64 > limit {
        return Err(ApiError::TooLarge {
            limit_mb: limit / (1024 * 1024),
        });
    }
    let image_type = ImageType::detect(&bytes).ok_or(ApiError::UnsupportedType)?;
    let sha256 = hex_sha256(&bytes);

    if let Some(image) = find_by_sha256(&state.db, &sha256).await? {
        return Ok(Ingested {
            image,
            duplicate: true,
        });
    }

    let (width, height) = match image_type.dimensions(&bytes) {
        Some((w, h)) => (Some(i64::from(w)), Some(i64::from(h))),
        None => (None, None),
    };
    let size = bytes.len() as i64;
    let created_file = write_file(&state.config.data_dir, &sha256, bytes).await?;

    let now = state.now();
    let record = NewImage {
        sha256: &sha256,
        size,
        mime: image_type.mime(),
        width,
        height,
        original_filename,
        uploaded_at: now.unix_timestamp(),
    };
    let facts = UploadFacts {
        image_type,
        original_filename,
        sha256: &sha256,
        uploaded_at: now,
    };
    match insert_image(state, &record, &facts).await {
        Ok(Some(id)) => Ok(Ingested {
            image: find_by_id(&state.db, id)
                .await?
                .context("image vanished after insert")?,
            duplicate: false,
        }),
        // Another upload of the same bytes won the race.
        Ok(None) => Ok(Ingested {
            image: find_by_sha256(&state.db, &sha256)
                .await?
                .context("duplicate image not found")?,
            duplicate: true,
        }),
        Err(e) => {
            if created_file {
                let _ = tokio::fs::remove_file(file_path(&state.config.data_dir, &sha256)).await;
            }
            Err(e)
        }
    }
}

struct NewImage<'a> {
    sha256: &'a str,
    size: i64,
    mime: &'a str,
    width: Option<i64>,
    height: Option<i64>,
    original_filename: &'a str,
    uploaded_at: i64,
}

/// Inserts the Image and its first (Primary) Path in one transaction.
/// Returns `None` if an Image with the same hash already exists.
async fn insert_image(
    state: &AppState,
    image: &NewImage<'_>,
    facts: &UploadFacts<'_>,
) -> Result<Option<i64>, ApiError> {
    let mut tx = state.db.begin().await?;
    let id = sqlx::query_scalar!(
        r#"INSERT INTO images (sha256, size, mime, width, height, original_filename, uploaded_at)
           VALUES (?, ?, ?, ?, ?, ?, ?)
           ON CONFLICT (sha256) DO NOTHING
           RETURNING id AS "id!""#,
        image.sha256,
        image.size,
        image.mime,
        image.width,
        image.height,
        image.original_filename,
        image.uploaded_at,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some(id) = id else {
        return Ok(None);
    };

    let template = state.path_template.as_ref();
    for _ in 0..MAX_PATH_DRAWS {
        let candidate = template.render(facts, state.random.as_ref());
        if is_reserved(&candidate) {
            continue;
        }
        let inserted = sqlx::query!(
            "INSERT INTO paths (path, image_id, is_primary, created_at) VALUES (?, ?, 1, ?)
             ON CONFLICT (path) DO NOTHING",
            candidate,
            id,
            image.uploaded_at,
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if inserted == 1 {
            tx.commit().await?;
            return Ok(Some(id));
        }
    }
    Err(anyhow::anyhow!(
        "no free Path after {MAX_PATH_DRAWS} draws of template {}",
        Template::as_str(template)
    )
    .into())
}

async fn find_by_sha256(db: &SqlitePool, sha256: &str) -> Result<Option<Image>, ApiError> {
    let id = sqlx::query_scalar!(r#"SELECT id AS "id!" FROM images WHERE sha256 = ?"#, sha256)
        .fetch_optional(db)
        .await?;
    match id {
        Some(id) => find_by_id(db, id).await,
        None => Ok(None),
    }
}

/// The Image with this id and its Paths.
pub async fn find_by_id(db: &SqlitePool, id: i64) -> Result<Option<Image>, ApiError> {
    let Some(row) = sqlx::query!(
        r#"SELECT id AS "id!", sha256, size, mime, width, height, original_filename, uploaded_at
           FROM images WHERE id = ?"#,
        id
    )
    .fetch_optional(db)
    .await?
    else {
        return Ok(None);
    };
    let paths = sqlx::query!(
        r#"SELECT path, is_primary AS "is_primary: bool" FROM paths
           WHERE image_id = ? ORDER BY is_primary DESC, id"#,
        id
    )
    .fetch_all(db)
    .await?
    .into_iter()
    .map(|p| ImagePath {
        path: p.path,
        is_primary: p.is_primary,
    })
    .collect();
    Ok(Some(Image {
        id: row.id,
        sha256: row.sha256,
        size: row.size,
        mime: row.mime,
        width: row.width,
        height: row.height,
        original_filename: row.original_filename,
        uploaded_at: row.uploaded_at,
        paths,
    }))
}

/// What is needed to serve a Path to a Visitor.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub sha256: String,
    pub mime: String,
    pub size: i64,
}

/// Maps a Path (exact, case-sensitive, without the leading `/`) to the live
/// Image it points at. Trashed Images are not served.
pub async fn resolve(db: &SqlitePool, path: &str) -> Result<Option<Resolved>, ApiError> {
    let row = sqlx::query_as!(
        Resolved,
        "SELECT i.sha256, i.mime, i.size FROM paths p JOIN images i ON i.id = p.image_id
         WHERE p.path = ? AND i.trashed_at IS NULL",
        path
    )
    .fetch_optional(db)
    .await?;
    Ok(row)
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Writes the file atomically unless it is already there. Returns whether
/// this call created it.
async fn write_file(data_dir: &Path, sha256: &str, bytes: Vec<u8>) -> anyhow::Result<bool> {
    let target = file_path(data_dir, sha256);
    let tmp_dir = data_dir.join("tmp");
    tokio::task::spawn_blocking(move || -> anyhow::Result<bool> {
        if target.is_file() {
            return Ok(false);
        }
        let parent = target.parent().expect("sharded path has a parent");
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
        std::fs::create_dir_all(&tmp_dir)
            .with_context(|| format!("creating {}", tmp_dir.display()))?;
        let mut tmp = tempfile::NamedTempFile::new_in(&tmp_dir).context("creating temp file")?;
        tmp.write_all(&bytes).context("writing upload")?;
        tmp.as_file().sync_all().context("syncing upload")?;
        tmp.persist(&target)
            .with_context(|| format!("moving upload to {}", target.display()))?;
        Ok(true)
    })
    .await
    .context("file write task")?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_prefixes() {
        for p in ["admin", "admin/", "admin/x.png", "api", "api/v1/info"] {
            assert!(is_reserved(p), "{p}");
        }
        for p in ["administrator.png", "apix", "x/admin/y", "Admin/x", "a.png"] {
            assert!(!is_reserved(p), "{p}");
        }
    }

    #[test]
    fn files_are_sharded_by_hash() {
        let sha = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
        assert_eq!(
            file_path(Path::new("/d"), sha),
            Path::new("/d/images/9f/86").join(sha)
        );
    }
}
