//! `api/v1/session`: log in, check the session, log out.

use axum::Json;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::auth::{self, Owner};
use crate::error::ApiResult;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub password: String,
    #[serde(default)]
    pub remember_me: bool,
}

#[derive(Serialize)]
pub struct SessionInfo {
    /// When the session ends, as unix seconds. Fixed at login.
    pub expires_at: i64,
}

impl SessionInfo {
    fn new(expires_at: OffsetDateTime) -> Self {
        Self {
            expires_at: expires_at.unix_timestamp(),
        }
    }
}

/// `POST /api/v1/session` (public): log in with the Owner's password.
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> ApiResult<Response> {
    let now = state.now();
    state.login_limiter.begin_attempt(now)?;
    auth::verify_password(&state.db, &req.password).await?;
    state.login_limiter.succeeded();

    let session = auth::create_session(&state.db, now, req.remember_me).await?;
    let cookie = auth::session_cookie(&session.token, session.expires_at - now);
    Ok((
        [(header::SET_COOKIE, cookie)],
        Json(SessionInfo::new(session.expires_at)),
    )
        .into_response())
}

/// `GET /api/v1/session`: the current session, or 401.
pub async fn current(owner: Owner) -> Json<SessionInfo> {
    Json(SessionInfo::new(owner.expires_at))
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
