//! `api/v1/session`: log in, check the session, log out, change the password
//! or the username.

use axum::Json;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::auth::{self, Owner};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub remember_me: bool,
}

#[derive(Serialize)]
pub struct SessionInfo {
    /// When the session ends, as unix seconds. Fixed at login.
    pub expires_at: i64,
    /// The Owner's username, as stored.
    pub username: Option<String>,
}

impl SessionInfo {
    async fn load(state: &AppState, expires_at: OffsetDateTime) -> ApiResult<Self> {
        Ok(Self {
            expires_at: expires_at.unix_timestamp(),
            username: auth::current_username(&state.db).await?,
        })
    }
}

/// `POST /api/v1/session` (public): log in with the Owner's username (any
/// case) and password. A wrong username and a wrong password get the same
/// answer, and both count toward the failed-login limit.
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> ApiResult<Response> {
    let now = state.now();
    state.login_limiter.begin_attempt(now)?;
    auth::verify_login(&state.db, &req.username, &req.password).await?;
    state.login_limiter.succeeded();

    let session = auth::create_session(&state.db, now, req.remember_me).await?;
    let cookie = auth::session_cookie(&session.token, session.expires_at - now);
    Ok((
        [(header::SET_COOKIE, cookie)],
        Json(SessionInfo::load(&state, session.expires_at).await?),
    )
        .into_response())
}

/// `GET /api/v1/session`: the current session, or 401.
pub async fn current(State(state): State<AppState>, owner: Owner) -> ApiResult<Json<SessionInfo>> {
    Ok(Json(SessionInfo::load(&state, owner.expires_at).await?))
}

/// `DELETE /api/v1/session`: log out.
pub async fn logout(State(state): State<AppState>, owner: Owner) -> ApiResult<Response> {
    auth::delete_session(&state.db, owner.session_id).await?;
    Ok((
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, auth::clear_session_cookie())],
    )
        .into_response())
}

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Serialize)]
pub struct PasswordChanged {
    /// How many other sessions were ended; the current one continues.
    pub ended_sessions: u64,
}

/// `PUT /api/v1/session/password`: change the Owner's password. Needs the
/// Owner's session (an API Token is refused) and the current password,
/// which counts toward the login rate limit like a login does. Every other
/// session ends; this one keeps its expiry. API Tokens keep working.
pub async fn change_password(
    State(state): State<AppState>,
    owner: Owner,
    Json(req): Json<ChangePasswordRequest>,
) -> ApiResult<Json<PasswordChanged>> {
    if !auth::password_long_enough(&req.new_password) {
        return Err(ApiError::PasswordTooShort);
    }
    let now = state.now();
    state.login_limiter.begin_attempt(now)?;
    auth::verify_current_password(&state.db, &req.current_password).await?;
    state.login_limiter.succeeded();

    let ended_sessions =
        auth::change_password(&state.db, &req.new_password, now, owner.session_id).await?;
    Ok(Json(PasswordChanged { ended_sessions }))
}

#[derive(Deserialize)]
pub struct ChangeUsernameRequest {
    pub current_password: String,
    pub username: String,
}

#[derive(Serialize)]
pub struct UsernameChanged {
    /// The new username, as stored (trimmed).
    pub username: String,
}

/// `PUT /api/v1/session/username`: change the Owner's username. Needs the
/// Owner's session and the current password, which counts toward the login
/// rate limit like a login does. No session ends.
pub async fn change_username(
    State(state): State<AppState>,
    _owner: Owner,
    Json(req): Json<ChangeUsernameRequest>,
) -> ApiResult<Json<UsernameChanged>> {
    let username = auth::clean_username(&req.username)?;
    let now = state.now();
    state.login_limiter.begin_attempt(now)?;
    auth::verify_current_password(&state.db, &req.current_password).await?;
    state.login_limiter.succeeded();

    auth::change_username(&state.db, &username, now).await?;
    Ok(Json(UsernameChanged { username }))
}
