use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::auth::Owner;
use crate::error::ApiError;
use crate::state::AppState;
use crate::store::{self, Stats};

#[derive(Serialize)]
pub struct Info {
    /// The server version; the admin frontend compares it with its own.
    pub version: &'static str,
    /// Image count and disk usage, only for the Owner.
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub stats: Option<Stats>,
}

/// `GET /api/v1/info` (public; the Library totals only with a session).
pub async fn info(
    State(state): State<AppState>,
    owner: Result<Owner, ApiError>,
) -> Result<Json<Info>, ApiError> {
    let stats = match owner {
        Ok(_) => Some(store::stats(&state.db).await?),
        Err(ApiError::Internal(e)) => return Err(ApiError::Internal(e)),
        Err(_) => None,
    };
    Ok(Json(Info {
        version: crate::VERSION,
        stats,
    }))
}
