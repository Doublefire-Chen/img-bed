//! The JSON API under the Reserved Prefix `api/`, versioned as `api/v1/`.
//!
//! Public endpoints: `GET info`, `POST session` (login). `POST images`
//! (upload) takes an Owner session or an API Token. Everything else,
//! including unknown paths, requires an Owner session (401 otherwise).

pub mod images;
mod info;
mod session;
mod settings;
mod tokens;
mod trash;

use axum::Router;
use axum::routing::{delete, get, post, put};

use crate::auth::Owner;
use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    let v1 = Router::new()
        .route("/info", get(info::info))
        .route(
            "/session",
            get(session::current)
                .post(session::login)
                .delete(session::logout),
        )
        .route("/session/password", put(session::change_password))
        .route("/session/username", put(session::change_username))
        .route("/images", get(images::list).post(images::upload))
        .route("/images/{id}", get(images::detail).delete(trash::trash_one))
        .route("/images/{id}/file", get(images::file))
        .route("/images/{id}/paths", post(images::add_path))
        .route("/images/{id}/paths/{*path}", delete(images::remove_path))
        .route("/images/{id}/primary", put(images::set_primary))
        .route("/trash", get(trash::list).post(trash::trash_many))
        .route("/trash/restore", post(trash::restore))
        .route("/trash/delete", post(trash::delete_many))
        .route("/trash/{id}", delete(trash::delete_one))
        .route("/settings", get(settings::get).put(settings::update))
        .route("/settings/preview", post(settings::preview))
        .route("/tokens", get(tokens::list).post(tokens::create))
        .route("/tokens/{id}", delete(tokens::revoke))
        .method_not_allowed_fallback(method_not_allowed);
    Router::new().nest("/v1", v1).fallback(not_found)
}

/// Unknown API paths answer 401 before 404, so the API reveals nothing to
/// someone without a session.
async fn not_found(_owner: Owner) -> ApiError {
    ApiError::NotFound
}

/// Likewise for a known path with the wrong method.
async fn method_not_allowed(_owner: Owner) -> ApiError {
    ApiError::MethodNotAllowed
}
