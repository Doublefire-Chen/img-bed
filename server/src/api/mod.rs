//! The JSON API under the Reserved Prefix `api/`, versioned as `api/v1/`.
//!
//! Public endpoints: `GET info`, `POST session` (login). Everything else,
//! including unknown paths, requires an Owner session (401 otherwise).

mod info;
mod session;

use axum::Router;
use axum::routing::get;

use crate::auth::Owner;
use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    let v1 = Router::new().route("/info", get(info::info)).route(
        "/session",
        get(session::current)
            .post(session::login)
            .delete(session::logout),
    );
    Router::new().nest("/v1", v1).fallback(not_found)
}

/// Unknown API paths answer 401 before 404, so the API reveals nothing to
/// someone without a session.
async fn not_found(_owner: Owner) -> ApiError {
    ApiError::NotFound
}
