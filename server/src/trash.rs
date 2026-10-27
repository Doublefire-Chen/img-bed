//! The Trash: deleting Images, Restoring them and Permanently Deleting them,
//! by the Owner or automatically [`TRASH_DAYS`] days after they were trashed.
//!
//! A trashed Image keeps its record, its file and all its Paths; only
//! serving stops (`store::resolve` skips it). Permanent Delete removes the
//! record (its Paths go with it, `ON DELETE CASCADE`) in one transaction and
//! then erases the file, while holding `AppState::files_lock` so an upload
//! of the same bytes waits instead of reusing a file about to disappear.

use std::collections::BTreeSet;

use anyhow::Context;
use sqlx::SqlitePool;
use time::Duration;

use crate::error::ApiError;
use crate::state::AppState;
use crate::store::{self, Image, ImageRow};

/// How long an Image stays in the Trash before it is Permanently Deleted.
pub const TRASH_DAYS: i64 = 30;
/// How often the server looks for Images due for Permanent Delete.
pub const PURGE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60 * 60);
/// The most Images one request may act on.
pub const MAX_BATCH: usize = 1000;

/// When an Image trashed at `trashed_at` (unix seconds) is Permanently
/// Deleted.
pub fn permanent_delete_at(trashed_at: i64) -> i64 {
    trashed_at + Duration::days(TRASH_DAYS).whole_seconds()
}

/// The distinct ids of a request, checked for size.
fn distinct_ids(ids: &[i64]) -> Result<(String, usize), ApiError> {
    let set: BTreeSet<i64> = ids.iter().copied().collect();
    if set.is_empty() {
        return Err(ApiError::BadRequest(
            "ids must list at least one Image".into(),
        ));
    }
    if set.len() > MAX_BATCH {
        return Err(ApiError::BadRequest(format!(
            "at most {MAX_BATCH} Images at a time"
        )));
    }
    let json = serde_json::to_string(&set).context("encoding image ids")?;
    Ok((json, set.len()))
}

/// Moves Images to the Trash at `now` (unix seconds). Images already in the
/// Trash keep their original date. All or nothing: an unknown id is
/// `NotFound` and nothing moves. Returns the Images in the order given.
pub async fn trash(db: &SqlitePool, ids: &[i64], now: i64) -> Result<Vec<Image>, ApiError> {
    let (json, wanted) = distinct_ids(ids)?;
    // Write first, so the transaction holds the write lock before it reads.
    let mut tx = db.begin().await?;
    sqlx::query!(
        "UPDATE images SET trashed_at = ?
         WHERE id IN (SELECT value FROM json_each(?)) AND trashed_at IS NULL",
        now,
        json,
    )
    .execute(&mut *tx)
    .await?;
    let found = count_existing(&mut tx, &json).await?;
    if found != wanted as i64 {
        return Err(ApiError::NotFound);
    }
    tx.commit().await?;
    find_all(db, ids).await
}

/// Restores Images from the Trash with all their Paths. Images that are not
/// in the Trash are left as they are. All or nothing: an unknown id
/// (including one Permanently Deleted meanwhile) is `NotFound`.
pub async fn restore(db: &SqlitePool, ids: &[i64]) -> Result<Vec<Image>, ApiError> {
    let (json, wanted) = distinct_ids(ids)?;
    let mut tx = db.begin().await?;
    sqlx::query!(
        "UPDATE images SET trashed_at = NULL
         WHERE id IN (SELECT value FROM json_each(?)) AND trashed_at IS NOT NULL",
        json,
    )
    .execute(&mut *tx)
    .await?;
    let found = count_existing(&mut tx, &json).await?;
    if found != wanted as i64 {
        return Err(ApiError::NotFound);
    }
    tx.commit().await?;
    find_all(db, ids).await
}

/// Permanently Deletes Images from the Trash: their records and Paths go in
/// one transaction (the Paths are free at once), then their files are
/// erased. All or nothing: an unknown id is `NotFound`, an Image that is not
/// in the Trash is `NotInTrash`. Returns how many were deleted.
pub async fn permanently_delete(state: &AppState, ids: &[i64]) -> Result<usize, ApiError> {
    let (json, wanted) = distinct_ids(ids)?;
    let _files = state.files_lock.lock().await;
    let mut tx = state.db.begin().await?;
    let deleted = sqlx::query_scalar!(
        "DELETE FROM images
         WHERE id IN (SELECT value FROM json_each(?)) AND trashed_at IS NOT NULL
         RETURNING sha256",
        json,
    )
    .fetch_all(&mut *tx)
    .await?;
    // Whatever is left of the ids was not in the Trash.
    let live = count_existing(&mut tx, &json).await?;
    if deleted.len() + live as usize != wanted {
        return Err(ApiError::NotFound);
    }
    if live > 0 {
        return Err(ApiError::NotInTrash(live as usize));
    }
    tx.commit().await?;
    erase_files(state, &deleted).await;
    Ok(deleted.len())
}

/// Permanently Deletes every Image trashed [`TRASH_DAYS`] days ago or
/// earlier (by the app clock). Returns how many were deleted. Runs at
/// startup and every [`PURGE_INTERVAL`] in `serve`; tests call it directly.
pub async fn purge_expired(state: &AppState) -> anyhow::Result<usize> {
    let cutoff = (state.now() - Duration::days(TRASH_DAYS)).unix_timestamp();
    let _files = state.files_lock.lock().await;
    let deleted = sqlx::query_scalar!(
        "DELETE FROM images WHERE trashed_at IS NOT NULL AND trashed_at <= ? RETURNING sha256",
        cutoff,
    )
    .fetch_all(&state.db)
    .await?;
    erase_files(state, &deleted).await;
    Ok(deleted.len())
}

/// Runs [`purge_expired`] now and then every [`PURGE_INTERVAL`], logging
/// what it deleted. Never returns.
pub async fn purge_periodically(state: AppState) {
    let mut interval = tokio::time::interval(PURGE_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        match purge_expired(&state).await {
            Ok(0) => tracing::debug!("Trash: nothing due for Permanent Delete"),
            Ok(n) => tracing::info!(
                "Trash: Permanently Deleted {n} {} trashed over {TRASH_DAYS} days ago",
                if n == 1 { "Image" } else { "Images" }
            ),
            Err(e) => tracing::error!("Trash: automatic Permanent Delete failed: {e:#}"),
        }
    }
}

/// Erases the files of Permanently Deleted Images. The records are already
/// gone, so a failure is logged rather than reported; a missing file is
/// fine.
async fn erase_files(state: &AppState, hashes: &[String]) {
    for sha256 in hashes {
        let path = store::file_path(&state.config.data_dir, sha256);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::warn!(
                    "file of Permanently Deleted Image was already gone: {}",
                    path.display()
                );
            }
            Err(e) => tracing::error!("could not erase {}: {e}", path.display()),
        }
    }
}

async fn count_existing(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    json: &str,
) -> Result<i64, ApiError> {
    Ok(sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "n!: i64" FROM images WHERE id IN (SELECT value FROM json_each(?))"#,
        json
    )
    .fetch_one(&mut **tx)
    .await?)
}

/// The Images with these ids, in the order given (duplicates once).
async fn find_all(db: &SqlitePool, ids: &[i64]) -> Result<Vec<Image>, ApiError> {
    let mut seen = BTreeSet::new();
    let mut images = Vec::new();
    for &id in ids {
        if seen.insert(id)
            && let Some(image) = store::find_by_id(db, id).await?
        {
            images.push(image);
        }
    }
    Ok(images)
}

/// One page of the Trash, most recently trashed first.
#[derive(Debug)]
pub struct TrashPage {
    pub images: Vec<Image>,
    /// Images in the Trash.
    pub total: i64,
}

pub async fn list(db: &SqlitePool, limit: i64, offset: i64) -> Result<TrashPage, ApiError> {
    let total = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "n!: i64" FROM images WHERE trashed_at IS NOT NULL"#
    )
    .fetch_one(db)
    .await?;
    let rows = sqlx::query_as!(
        ImageRow,
        r#"SELECT id AS "id!", sha256, size, mime, width, height, original_filename,
                  uploaded_at, trashed_at
           FROM images WHERE trashed_at IS NOT NULL
           ORDER BY trashed_at DESC, id DESC
           LIMIT ? OFFSET ?"#,
        limit,
        offset
    )
    .fetch_all(db)
    .await?;
    Ok(TrashPage {
        images: store::with_paths(db, rows).await?,
        total,
    })
}
