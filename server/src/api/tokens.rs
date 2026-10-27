//! API Tokens: `GET`/`POST /api/v1/tokens` and `DELETE /api/v1/tokens/{id}`
//! (revoke). Owner session for all; an API Token can't manage tokens.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::auth::Owner;
use crate::error::ApiError;
use crate::state::AppState;
use crate::tokens::{self, ApiToken};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateRequest {
    pub name: String,
}

#[derive(Serialize)]
pub struct CreatedToken {
    #[serde(flatten)]
    pub token: ApiToken,
    /// The secret to send as `Authorization: Bearer <secret>`. Returned only
    /// here, once; it can't be shown again.
    pub secret: String,
}

#[derive(Serialize)]
pub struct TokenList {
    /// Active tokens first, then revoked ones; newest first within each.
    pub tokens: Vec<ApiToken>,
}

/// `GET /api/v1/tokens`: every token, without secrets.
pub async fn list(
    State(state): State<AppState>,
    _owner: Owner,
) -> Result<Json<TokenList>, ApiError> {
    Ok(Json(TokenList {
        tokens: tokens::list(&state.db).await?,
    }))
}

/// `POST /api/v1/tokens` `{"name"}`: creates a token and answers it with its
/// secret (201).
pub async fn create(
    State(state): State<AppState>,
    _owner: Owner,
    body: Result<Json<CreateRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<CreatedToken>), ApiError> {
    let Json(req) = body.map_err(|e| ApiError::BadRequest(e.body_text()))?;
    let new = tokens::create(&state.db, &req.name, state.now()).await?;
    Ok((
        StatusCode::CREATED,
        Json(CreatedToken {
            token: new.token,
            secret: new.secret,
        }),
    ))
}

/// `DELETE /api/v1/tokens/{id}`: revokes a token and answers it with
/// `revoked_at` set. Revoking it again keeps the first date.
pub async fn revoke(
    State(state): State<AppState>,
    _owner: Owner,
    Path(id): Path<String>,
) -> Result<Json<ApiToken>, ApiError> {
    let id: i64 = id.parse().map_err(|_| ApiError::NotFound)?;
    Ok(Json(tokens::revoke(&state.db, id, state.now()).await?))
}
