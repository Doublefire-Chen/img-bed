//! Owner authentication: the password, login sessions, and the failed-login
//! rate limit.
//!
//! - The password hash (argon2id) lives in the database and is set only by
//!   `img-bed set-password` (see [`set_password`]). Setting it ends every
//!   session.
//! - A session is a random token in an `HttpOnly; Secure; SameSite=Strict`
//!   cookie; only its SHA-256 is stored. It expires a fixed time after login
//!   ([`SESSION_TTL`], or [`REMEMBER_ME_TTL`]) and never slides.
//! - Handlers that need the Owner take an [`Owner`] argument; extracting it
//!   fails with 401 when there is no valid session. [`Owner`] ignores
//!   `Authorization` headers, so an API Token is refused everywhere it is
//!   used.
//! - The upload endpoint alone takes an [`Uploader`]: the Owner's session or
//!   an active API Token (`Authorization: Bearer <secret>`).

use std::collections::VecDeque;
use std::sync::Mutex;

use anyhow::Context;
use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use axum::extract::FromRequestParts;
use axum::http::header;
use axum::http::request::Parts;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use time::{Duration, OffsetDateTime};

use crate::error::ApiError;
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "img_bed_session";
pub const SESSION_TTL: Duration = Duration::hours(24);
pub const REMEMBER_ME_TTL: Duration = Duration::days(7);
pub const MIN_PASSWORD_LEN: usize = 8;

/// Failed logins allowed within [`LOGIN_WINDOW`] before further attempts are
/// refused with 429.
pub const MAX_FAILED_LOGINS: usize = 5;
pub const LOGIN_WINDOW: Duration = Duration::minutes(15);

// ---------------------------------------------------------------------------
// Password

/// Sets (or replaces) the Owner's password and ends every existing session.
pub async fn set_password(
    db: &SqlitePool,
    password: &str,
    now: OffsetDateTime,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        password.chars().count() >= MIN_PASSWORD_LEN,
        "the password must be at least {MIN_PASSWORD_LEN} characters"
    );
    let password = password.to_owned();
    let hash = tokio::task::spawn_blocking(move || hash_password(&password)).await??;
    let now = now.unix_timestamp();

    let mut tx = db.begin().await?;
    sqlx::query!(
        "INSERT INTO owner (id, password_hash, updated_at) VALUES (1, ?, ?)
         ON CONFLICT (id) DO UPDATE SET password_hash = excluded.password_hash,
                                        updated_at = excluded.updated_at",
        hash,
        now
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!("DELETE FROM sessions")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("hashing password: {e}"))?;
    Ok(hash.to_string())
}

/// Checks `password` against the stored hash.
pub async fn verify_password(db: &SqlitePool, password: &str) -> Result<(), ApiError> {
    let row = sqlx::query!("SELECT password_hash FROM owner WHERE id = 1")
        .fetch_optional(db)
        .await?;
    let Some(row) = row else {
        return Err(ApiError::NoPassword);
    };
    let password = password.to_owned();
    let ok = tokio::task::spawn_blocking(move || -> anyhow::Result<bool> {
        let parsed = PasswordHash::new(&row.password_hash)
            .map_err(|e| anyhow::anyhow!("stored password hash is invalid: {e}"))?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    })
    .await
    .context("password verification task")??;
    if ok {
        Ok(())
    } else {
        Err(ApiError::WrongPassword)
    }
}

// ---------------------------------------------------------------------------
// Sessions

/// A newly created session: the cookie token (secret) and its expiry.
pub struct NewSession {
    pub token: String,
    pub expires_at: OffsetDateTime,
}

pub async fn create_session(
    db: &SqlitePool,
    now: OffsetDateTime,
    remember_me: bool,
) -> Result<NewSession, ApiError> {
    let ttl = if remember_me {
        REMEMBER_ME_TTL
    } else {
        SESSION_TTL
    };
    let expires_at = now + ttl;
    // Secrets always come from the OS CSPRNG, never the injected random source.
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let token = URL_SAFE_NO_PAD.encode(bytes);
    let token_hash = hash_token(&token);
    let (created, expires) = (now.unix_timestamp(), expires_at.unix_timestamp());

    // Opportunistically drop expired sessions; they are rejected anyway.
    sqlx::query!("DELETE FROM sessions WHERE expires_at <= ?", created)
        .execute(db)
        .await?;
    sqlx::query!(
        "INSERT INTO sessions (token_hash, created_at, expires_at) VALUES (?, ?, ?)",
        token_hash,
        created,
        expires
    )
    .execute(db)
    .await?;
    Ok(NewSession { token, expires_at })
}

pub async fn delete_session(db: &SqlitePool, session_id: i64) -> Result<(), ApiError> {
    sqlx::query!("DELETE FROM sessions WHERE id = ?", session_id)
        .execute(db)
        .await?;
    Ok(())
}

fn hash_token(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// `Set-Cookie` value that stores the session token in the browser.
pub fn session_cookie(token: &str, max_age: Duration) -> String {
    format!(
        "{SESSION_COOKIE}={token}; Path=/; Max-Age={}; HttpOnly; Secure; SameSite=Strict",
        max_age.whole_seconds().max(0)
    )
}

/// `Set-Cookie` value that removes the session cookie.
pub fn clear_session_cookie() -> String {
    session_cookie("", Duration::ZERO)
}

fn session_token_from(parts: &Parts) -> Option<&str> {
    parts
        .headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == SESSION_COOKIE)
        .map(|(_, value)| value)
        .filter(|value| !value.is_empty())
}

/// Proof that the request carries a valid Owner session. Take it as a
/// handler argument to make an endpoint require login (401 otherwise).
#[derive(Debug, Clone)]
pub struct Owner {
    pub session_id: i64,
    pub expires_at: OffsetDateTime,
}

impl FromRequestParts<AppState> for Owner {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let token = session_token_from(parts).ok_or(ApiError::Unauthorized)?;
        let token_hash = hash_token(token);
        let now = state.now().unix_timestamp();
        let row = sqlx::query!(
            r#"SELECT id AS "id!", expires_at FROM sessions WHERE token_hash = ? AND expires_at > ?"#,
            token_hash,
            now
        )
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::Unauthorized)?;
        let expires_at = OffsetDateTime::from_unix_timestamp(row.expires_at)
            .context("session expiry out of range")?;
        Ok(Owner {
            session_id: row.id,
            expires_at,
        })
    }
}

/// Who is uploading: the Owner in the browser, or a tool with an API Token.
/// Only the upload endpoint takes this; every other endpoint takes [`Owner`].
///
/// A request with an `Authorization` header is judged by it alone: it must be
/// `Bearer <secret>` of an active API Token (401 `invalid_token` otherwise),
/// and a session cookie next to it is not consulted. Without the header the
/// Owner's session is required, as for [`Owner`].
#[derive(Debug, Clone)]
pub enum Uploader {
    Owner(Owner),
    ApiToken { token_id: i64 },
}

impl FromRequestParts<AppState> for Uploader {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let Some(value) = parts.headers.get(header::AUTHORIZATION) else {
            return Ok(Uploader::Owner(
                Owner::from_request_parts(parts, state).await?,
            ));
        };
        let secret = value
            .to_str()
            .ok()
            .and_then(|v| v.split_once(' '))
            .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
            .map(|(_, secret)| secret.trim())
            .ok_or(ApiError::InvalidToken)?;
        let token_id = crate::tokens::authenticate(&state.db, secret, state.now())
            .await?
            .ok_or(ApiError::InvalidToken)?;
        Ok(Uploader::ApiToken { token_id })
    }
}

// ---------------------------------------------------------------------------
// Failed-login rate limit

/// Limits login attempts across the whole instance (there is one Owner, so
/// there is one account to protect). Every attempt is counted as a failure
/// up front and only forgiven once it succeeds, so concurrent guesses cannot
/// slip past the limit while argon2 is busy. Kept in memory: a restart resets
/// it, which is acceptable for a single-Owner host.
#[derive(Debug, Default)]
pub struct LoginLimiter {
    attempts: Mutex<VecDeque<OffsetDateTime>>,
}

impl LoginLimiter {
    /// Registers an attempt, or refuses it if too many recent attempts failed.
    pub fn begin_attempt(&self, now: OffsetDateTime) -> Result<(), ApiError> {
        let mut attempts = self.attempts.lock().unwrap();
        while attempts.front().is_some_and(|t| *t + LOGIN_WINDOW <= now) {
            attempts.pop_front();
        }
        if attempts.len() >= MAX_FAILED_LOGINS {
            let oldest = *attempts.front().expect("non-empty");
            let wait = (oldest + LOGIN_WINDOW - now).whole_seconds().max(1);
            return Err(ApiError::TooManyAttempts {
                retry_after_secs: wait as u64,
            });
        }
        attempts.push_back(now);
        Ok(())
    }

    /// Forgives the failures once the Owner has proven the password.
    pub fn succeeded(&self) {
        self.attempts.lock().unwrap().clear();
    }
}
