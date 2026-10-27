//! The Trash: `DELETE /api/v1/images/{id}`, `POST /api/v1/trash` (delete
//! several), `GET /api/v1/trash`, `POST /api/v1/trash/restore`,
//! `POST /api/v1/trash/delete` and `DELETE /api/v1/trash/{id}` (Permanent
//! Delete). Owner session for all.

use axum::Json;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};

use super::images::{DEFAULT_PER_PAGE, ImageView, MAX_PER_PAGE, image_id};
use crate::auth::Owner;
use crate::error::ApiError;
use crate::state::AppState;
use crate::store::Image;
use crate::trash;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdsRequest {
    pub ids: Vec<i64>,
}

fn ids(body: Result<Json<IdsRequest>, JsonRejection>) -> Result<Vec<i64>, ApiError> {
    body.map(|Json(req)| req.ids)
        .map_err(|e| ApiError::BadRequest(e.body_text()))
}

#[derive(Serialize)]
pub struct ImagesResponse {
    pub images: Vec<ImageView>,
}

fn images_response(state: &AppState, images: Vec<Image>) -> Json<ImagesResponse> {
    let public_url = &state.config.public_url;
    Json(ImagesResponse {
        images: images
            .into_iter()
            .map(|image| ImageView::new(image, public_url))
            .collect(),
    })
}

/// `DELETE /api/v1/images/{id}`: moves one Image to the Trash. Answers the
/// Image with `trashed_at` and `permanent_delete_at` set. Deleting an Image
/// already in the Trash changes nothing.
pub async fn trash_one(
    State(state): State<AppState>,
    _owner: Owner,
    Path(id): Path<String>,
) -> Result<Json<ImageView>, ApiError> {
    let id = image_id(&id)?;
    let now = state.now().unix_timestamp();
    let image = trash::trash(&state.db, &[id], now)
        .await?
        .pop()
        .ok_or(ApiError::NotFound)?;
    Ok(Json(ImageView::new(image, &state.config.public_url)))
}

/// `POST /api/v1/trash` with `{"ids": [..]}`: moves Images to the Trash, all
/// or none (404 if any id is unknown). Answers `{"images": [..]}`.
pub async fn trash_many(
    State(state): State<AppState>,
    _owner: Owner,
    body: Result<Json<IdsRequest>, JsonRejection>,
) -> Result<Json<ImagesResponse>, ApiError> {
    let ids = ids(body)?;
    let now = state.now().unix_timestamp();
    let images = trash::trash(&state.db, &ids, now).await?;
    Ok(images_response(&state, images))
}

#[derive(Deserialize)]
pub struct ListQuery {
    page: Option<i64>,
    per_page: Option<i64>,
}

#[derive(Serialize)]
pub struct ListResponse {
    pub images: Vec<ImageView>,
    pub page: i64,
    pub per_page: i64,
    /// Images in the Trash.
    pub total: i64,
    pub pages: i64,
    /// Days an Image stays in the Trash before it is Permanently Deleted.
    pub trash_days: i64,
}

/// `GET /api/v1/trash?page=&per_page=`: the Trash, most recently trashed
/// first. Each Image carries `trashed_at` and `permanent_delete_at`.
pub async fn list(
    State(state): State<AppState>,
    _owner: Owner,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<ListResponse>, ApiError> {
    let Query(query) = query.map_err(|e| ApiError::BadRequest(e.body_text()))?;
    let page = query.page.unwrap_or(1);
    if page < 1 {
        return Err(ApiError::BadRequest("page must be 1 or more".into()));
    }
    let per_page = query.per_page.unwrap_or(DEFAULT_PER_PAGE);
    if !(1..=MAX_PER_PAGE).contains(&per_page) {
        return Err(ApiError::BadRequest(format!(
            "per_page must be between 1 and {MAX_PER_PAGE}"
        )));
    }
    let offset = (page - 1).saturating_mul(per_page);
    let result = trash::list(&state.db, per_page, offset).await?;
    let public_url = &state.config.public_url;
    Ok(Json(ListResponse {
        images: result
            .images
            .into_iter()
            .map(|image| ImageView::new(image, public_url))
            .collect(),
        page,
        per_page,
        total: result.total,
        pages: (result.total + per_page - 1) / per_page,
        trash_days: trash::TRASH_DAYS,
    }))
}

/// `POST /api/v1/trash/restore` with `{"ids": [..]}`: Restores Images with
/// all their Paths, all or none (404 if any id is unknown, e.g. already
/// Permanently Deleted). Images not in the Trash are left as they are.
pub async fn restore(
    State(state): State<AppState>,
    _owner: Owner,
    body: Result<Json<IdsRequest>, JsonRejection>,
) -> Result<Json<ImagesResponse>, ApiError> {
    let ids = ids(body)?;
    let images = trash::restore(&state.db, &ids).await?;
    Ok(images_response(&state, images))
}

#[derive(Serialize)]
pub struct DeletedResponse {
    /// How many Images were Permanently Deleted.
    pub deleted: usize,
}

/// `POST /api/v1/trash/delete` with `{"ids": [..]}`: Permanently Deletes
/// Images from the Trash, all or none: 404 if any id is unknown, 409
/// `not_in_trash` if any Image is not in the Trash.
pub async fn delete_many(
    State(state): State<AppState>,
    _owner: Owner,
    body: Result<Json<IdsRequest>, JsonRejection>,
) -> Result<Json<DeletedResponse>, ApiError> {
    let ids = ids(body)?;
    let deleted = trash::permanently_delete(&state, &ids).await?;
    Ok(Json(DeletedResponse { deleted }))
}

/// `DELETE /api/v1/trash/{id}`: Permanently Deletes one Image from the
/// Trash.
pub async fn delete_one(
    State(state): State<AppState>,
    _owner: Owner,
    Path(id): Path<String>,
) -> Result<Json<DeletedResponse>, ApiError> {
    let id = image_id(&id)?;
    let deleted = trash::permanently_delete(&state, &[id]).await?;
    Ok(Json(DeletedResponse { deleted }))
}
