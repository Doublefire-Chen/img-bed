//! Builds the HTTP router. Used by `serve` and, in-process, by the tests.

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use tower_http::trace::TraceLayer;

use crate::state::AppState;
use crate::{api, frontend};

pub fn router(state: AppState) -> Router {
    let body_limit = usize::try_from(state.config.upload_limit_bytes).unwrap_or(usize::MAX);
    Router::new()
        .nest("/api", api::router())
        .merge(frontend::router(&state.config.frontend_dir))
        // Image Paths are served here in a later slice.
        .fallback(|| async { StatusCode::NOT_FOUND })
        .layer(DefaultBodyLimit::max(body_limit))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
