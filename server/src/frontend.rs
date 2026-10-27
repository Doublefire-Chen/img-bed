//! Serves the separately built admin frontend from `frontend_dir` under the
//! Reserved Prefix `admin/`. Unknown paths fall back to `index.html` so deep
//! links into the single-page app work.
//!
//! The frontend files are public (the login screen is part of them); they
//! contain no Owner data, which is only reachable through the API.

use std::path::Path;

use axum::Router;
use axum::extract::Request;
use axum::http::{HeaderValue, Uri, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};

use crate::state::AppState;

pub const PREFIX: &str = "/admin";

pub fn router(frontend_dir: &Path) -> Router<AppState> {
    let index = ServeFile::new(frontend_dir.join("index.html"));
    let files = ServeDir::new(frontend_dir).fallback(index);
    let serve = move |req: Request| serve_file(files.clone(), req);
    Router::new()
        .route(PREFIX, get(|| async { Redirect::permanent("/admin/") }))
        .route("/admin/", get(serve.clone()).head(serve.clone()))
        .route("/admin/{*path}", get(serve.clone()).head(serve))
}

async fn serve_file(files: ServeDir<ServeFile>, mut req: Request) -> Response {
    let inner = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/")
        .strip_prefix(PREFIX)
        .unwrap_or("/")
        .to_owned();
    let immutable = inner.starts_with("/assets/");
    *req.uri_mut() = inner.parse().unwrap_or_else(|_| Uri::from_static("/"));

    let mut response = match files.oneshot(req).await {
        Ok(response) => response.into_response(),
        Err(never) => match never {},
    };
    // Vite puts content-hashed files under `assets/`; they never change, so
    // they may be cached forever. Everything else (notably `index.html`) must
    // be revalidated so a new deployment shows up immediately.
    let cache = if immutable && response.status().is_success() {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    response
}
