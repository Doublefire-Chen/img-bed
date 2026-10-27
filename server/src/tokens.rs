//! API Tokens: named, revocable secrets that let a tool (PicGo, ShareX, a
//! script) upload on the Owner's behalf.
//!
//! - A secret is `imgbed_` followed by 32 bytes from the OS CSPRNG (never the
//!   injected random source), base64url-encoded. It is returned once, by
//!   [`create`]; only its SHA-256 is stored. A fast hash is enough for a
//!   256-bit random secret, and it lets [`authenticate`] find the token by
//!   an indexed lookup on the hash instead of comparing secrets.
//! - A token authorises only the upload endpoint (see [`crate::auth::Uploader`]).
//! - Revoked tokens stay listed (shown as revoked) and are refused. Names are
//!   unique among active tokens, so a revoked name can be reused.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use time::OffsetDateTime;

use crate::error::ApiError;

/// Every secret starts with this, so a leaked one is recognisable (e.g. by
/// secret scanners) and can't be mistaken for a session token.
pub const SECRET_PREFIX: &str = "imgbed_";
/// Random bytes in a secret.
pub const SECRET_BYTES: usize = 32;
/// The longest accepted token name, in characters.
pub const MAX_NAME_CHARS: usize = 100;
/// `last_used_at` is written at most this often per token, so a busy tool
/// doesn't cost a database write per upload.
pub const LAST_USED_RESOLUTION_SECS: i64 = 60;

/// An API Token as listed in the admin area. Never carries the secret.
#[derive(Debug, Clone, Serialize)]
pub struct ApiToken {
    pub id: i64,
    pub name: String,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds; `null` until the token is first used. Updated at most
    /// once a minute.
    pub last_used_at: Option<i64>,
    /// Unix seconds; `null` while the token is active.
    pub revoked_at: Option<i64>,
}

/// A newly created token and its secret, which is never available again.
pub struct NewToken {
    pub token: ApiToken,
    pub secret: String,
}

fn hash_secret(secret: &str) -> String {
    Sha256::digest(secret.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn new_secret() -> String {
    let mut bytes = [0u8; SECRET_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    format!("{SECRET_PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes))
}

/// Checks and normalises a token name: trimmed, not blank, at most
/// [`MAX_NAME_CHARS`] characters, no control characters.
fn clean_name(name: &str) -> Result<String, ApiError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ApiError::BadRequest("give the API Token a name".into()));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(ApiError::BadRequest(format!(
            "the name is longer than {MAX_NAME_CHARS} characters"
        )));
    }
    if name.chars().any(char::is_control) {
        return Err(ApiError::BadRequest(
            "the name can't contain control characters".into(),
        ));
    }
    Ok(name.to_owned())
}

/// Creates a token named `name` and returns it with its secret.
pub async fn create(
    db: &SqlitePool,
    name: &str,
    now: OffsetDateTime,
) -> Result<NewToken, ApiError> {
    let name = clean_name(name)?;
    let secret = new_secret();
    let secret_hash = hash_secret(&secret);
    let now = now.unix_timestamp();
    let id = sqlx::query_scalar!(
        r#"INSERT INTO api_tokens (name, secret_hash, created_at) VALUES (?, ?, ?)
           ON CONFLICT DO NOTHING
           RETURNING id AS "id!""#,
        name,
        secret_hash,
        now
    )
    .fetch_optional(db)
    .await?;
    // The only realistic conflict is the active-name index (a 256-bit hash
    // collision is not).
    let Some(id) = id else {
        return Err(ApiError::TokenNameTaken(name));
    };
    Ok(NewToken {
        token: ApiToken {
            id,
            name,
            created_at: now,
            last_used_at: None,
            revoked_at: None,
        },
        secret,
    })
}

/// Every token, active ones first, then newest first.
pub async fn list(db: &SqlitePool) -> Result<Vec<ApiToken>, ApiError> {
    Ok(sqlx::query_as!(
        ApiToken,
        r#"SELECT id AS "id!", name, created_at, last_used_at, revoked_at
           FROM api_tokens
           ORDER BY revoked_at IS NOT NULL, created_at DESC, id DESC"#
    )
    .fetch_all(db)
    .await?)
}

/// Revokes a token; it is refused from now on. Revoking a revoked token
/// keeps its original date. 404 for an unknown id.
pub async fn revoke(db: &SqlitePool, id: i64, now: OffsetDateTime) -> Result<ApiToken, ApiError> {
    let now = now.unix_timestamp();
    sqlx::query_as!(
        ApiToken,
        r#"UPDATE api_tokens SET revoked_at = COALESCE(revoked_at, ?)
           WHERE id = ?
           RETURNING id AS "id!", name, created_at, last_used_at, revoked_at"#,
        now,
        id
    )
    .fetch_optional(db)
    .await?
    .ok_or(ApiError::NotFound)
}

/// Finds the active token whose secret is `secret` and records the use.
/// `None` for an unknown or revoked secret.
pub async fn authenticate(
    db: &SqlitePool,
    secret: &str,
    now: OffsetDateTime,
) -> Result<Option<i64>, ApiError> {
    if !secret.starts_with(SECRET_PREFIX) {
        return Ok(None);
    }
    let secret_hash = hash_secret(secret);
    let id = sqlx::query_scalar!(
        r#"SELECT id AS "id!" FROM api_tokens WHERE secret_hash = ? AND revoked_at IS NULL"#,
        secret_hash
    )
    .fetch_optional(db)
    .await?;
    if let Some(id) = id {
        let now = now.unix_timestamp();
        let stale = now - LAST_USED_RESOLUTION_SECS;
        sqlx::query!(
            "UPDATE api_tokens SET last_used_at = ?
             WHERE id = ? AND (last_used_at IS NULL OR last_used_at <= ?)",
            now,
            id,
            stale
        )
        .execute(db)
        .await?;
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_prefixed_long_and_distinct() {
        let a = new_secret();
        let b = new_secret();
        assert!(a.starts_with(SECRET_PREFIX));
        assert_eq!(a.len(), SECRET_PREFIX.len() + 43); // 32 bytes in base64url
        assert_ne!(a, b);
    }

    #[test]
    fn names_are_trimmed_and_checked() {
        assert_eq!(clean_name("  PicGo ").unwrap(), "PicGo");
        assert!(clean_name("   ").is_err());
        assert!(clean_name("a\nb").is_err());
        assert!(clean_name(&"x".repeat(MAX_NAME_CHARS)).is_ok());
        assert!(clean_name(&"x".repeat(MAX_NAME_CHARS + 1)).is_err());
    }
}
