//! Builds the HTTP router. Used by `serve` and, in-process, by the tests.

use axum::Router;
use axum::extract::DefaultBodyLimit;
use tower_http::trace::TraceLayer;

use crate::state::AppState;
use crate::{api, frontend, serve};

/// Room on top of the upload limit for the multipart framing, so a file of
/// exactly the limit still fits in one request.
pub const BODY_OVERHEAD_BYTES: u64 = 1024 * 1024;

pub fn router(state: AppState) -> Router {
    let body_limit = state
        .config
        .upload_limit_bytes
        .saturating_add(BODY_OVERHEAD_BYTES);
    let body_limit = usize::try_from(body_limit).unwrap_or(usize::MAX);
    Router::new()
        .nest("/api", api::router())
        .merge(frontend::router(&state.config.frontend_dir))
        // Everything outside the Reserved Prefixes is an Image Path.
        .fallback(serve::serve_image)
        .layer(DefaultBodyLimit::max(body_limit))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
