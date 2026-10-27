//! `api/v1/images`: uploading Images, the Library list, Image detail and
//! Path management.

use axum::Json;
use axum::extract::multipart::{Multipart, MultipartError, MultipartRejection};
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::auth::Owner;
use crate::error::ApiError;
use crate::path;
use crate::serve::url_for;
use crate::state::AppState;
use crate::store::{self, Image};

#[derive(Serialize)]
pub struct PathView {
    pub path: String,
    pub url: String,
    pub is_primary: bool,
}

#[derive(Serialize)]
pub struct ImageView {
    pub id: i64,
    pub sha256: String,
    pub size: i64,
    /// The detected MIME type, e.g. `image/png`.
    #[serde(rename = "type")]
    pub mime: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub original_filename: String,
    /// Unix seconds.
    pub uploaded_at: i64,
    /// Unix seconds; `null` unless the Image is in the Trash.
    pub trashed_at: Option<i64>,
    /// The Primary Path and its full URL.
    pub path: String,
    pub url: String,
    pub paths: Vec<PathView>,
}

impl ImageView {
    pub fn new(image: Image, public_url: &str) -> Self {
        let paths: Vec<PathView> = image
            .paths
            .into_iter()
            .map(|p| PathView {
                url: url_for(public_url, &p.path),
                path: p.path,
                is_primary: p.is_primary,
            })
            .collect();
        let primary = paths
            .iter()
            .find(|p| p.is_primary)
            .or(paths.first())
            .map(|p| (p.path.clone(), p.url.clone()))
            .unwrap_or_default();
        Self {
            id: image.id,
            sha256: image.sha256,
            size: image.size,
            mime: image.mime,
            width: image.width,
            height: image.height,
            original_filename: image.original_filename,
            uploaded_at: image.uploaded_at,
            trashed_at: image.trashed_at,
            path: primary.0,
            url: primary.1,
            paths,
        }
    }
}

/// The outcome for one uploaded file.
#[derive(Serialize)]
#[serde(untagged)]
pub enum UploadResult {
    Stored {
        filename: String,
        /// A Duplicate Upload: the bytes matched an existing Image, which is
        /// returned unchanged with its existing Paths. Nothing was created.
        duplicate: bool,
        /// For a Duplicate Upload, when the existing Image was first
        /// uploaded (unix seconds); `null` for a new Image.
        original_uploaded_at: Option<i64>,
        image: Box<ImageView>,
    },
    Failed {
        filename: String,
        error: serde_json::Value,
    },
}

/// `POST /api/v1/images`: upload one or more files as `multipart/form-data`
/// (every part with a filename, or named `file`, is a file).
///
/// Each file is handled on its own and gets one entry in `results`, in
/// order. The status is 200 when at least one file was stored; when none
/// was, it is the status of the first failure (e.g. 413 or 415) and the
/// body also carries that failure as the usual top-level `error`.
pub async fn upload(
    State(state): State<AppState>,
    _owner: Owner,
    multipart: Result<Multipart, MultipartRejection>,
) -> Result<Response, ApiError> {
    let mut multipart = multipart.map_err(|e| ApiError::BadRequest(e.body_text()))?;
    let limit = state.config.upload_limit_bytes;
    let too_large = || ApiError::TooLarge {
        limit_mb: limit / (1024 * 1024),
    };

    let mut results = Vec::new();
    let mut first_error: Option<ApiError> = None;
    let mut any_stored = false;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| multipart_error(e, limit))?
    {
        let is_file = field.file_name().is_some() || field.name() == Some("file");
        if !is_file {
            continue;
        }
        let filename = field.file_name().unwrap_or_default().to_owned();

        // Read the part, refusing to buffer more than the limit.
        let mut bytes = Vec::new();
        let mut oversize = false;
        while let Some(chunk) = field.chunk().await.map_err(|e| multipart_error(e, limit))? {
            if oversize {
                continue;
            }
            if (bytes.len() + chunk.len()) as u64 > limit {
                oversize = true;
                bytes = Vec::new();
            } else {
                bytes.extend_from_slice(&chunk);
            }
        }

        let outcome = if oversize {
            Err(too_large())
        } else {
            store::ingest(&state, &filename, bytes).await
        };
        match outcome {
            Ok(ingested) => {
                any_stored = true;
                let original_uploaded_at = ingested.duplicate.then_some(ingested.image.uploaded_at);
                results.push(UploadResult::Stored {
                    filename,
                    duplicate: ingested.duplicate,
                    original_uploaded_at,
                    image: Box::new(ImageView::new(ingested.image, &state.config.public_url)),
                });
            }
            Err(ApiError::Internal(e)) => return Err(ApiError::Internal(e)),
            Err(e) => {
                let (_, code) = e.status_and_code();
                results.push(UploadResult::Failed {
                    filename,
                    error: json!({ "code": code, "message": e.to_string() }),
                });
                first_error.get_or_insert(e);
            }
        }
    }

    if results.is_empty() {
        return Err(ApiError::BadRequest(
            "no files in the upload; send them as multipart/form-data".into(),
        ));
    }
    match first_error {
        Some(e) if !any_stored => {
            let (status, code) = e.status_and_code();
            let body = json!({
                "error": { "code": code, "message": e.to_string() },
                "results": results,
            });
            Ok((status, Json(body)).into_response())
        }
        _ => Ok((StatusCode::OK, Json(json!({ "results": results }))).into_response()),
    }
}

/// A broken or over-long request body. Exceeding the body limit (the upload
/// limit plus room for the form overhead) answers 413 like an oversize file.
fn multipart_error(e: MultipartError, limit: u64) -> ApiError {
    if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
        ApiError::TooLarge {
            limit_mb: limit / (1024 * 1024),
        }
    } else {
        ApiError::BadRequest(format!("invalid upload: {}", e.body_text()))
    }
}

/// Images per Library page unless `per_page` says otherwise.
pub const DEFAULT_PER_PAGE: i64 = 60;
/// The largest accepted `per_page`.
pub const MAX_PER_PAGE: i64 = 200;

#[derive(Deserialize)]
pub struct ListQuery {
    /// 1-based page number.
    page: Option<i64>,
    per_page: Option<i64>,
    /// Keeps Images whose original filename or any Path contains this text
    /// (ASCII case ignored). Blank means no search.
    q: Option<String>,
}

#[derive(Serialize)]
pub struct ListResponse {
    pub images: Vec<ImageView>,
    pub page: i64,
    pub per_page: i64,
    /// Images matching the search across all pages.
    pub total: i64,
    /// Pages for `total` at `per_page` (0 when nothing matches).
    pub pages: i64,
    /// Library totals, independent of the search.
    pub stats: store::Stats,
}

/// `GET /api/v1/images?page=&per_page=&q=`: the Library, newest first.
///
/// Offset paging by page number: the Owner jumps between numbered pages,
/// and the Library changes rarely enough that an upload shifting a page by
/// one Image is harmless. Trashed Images are left out.
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
    let search = query.q.as_deref().map(str::trim).filter(|q| !q.is_empty());
    let offset = (page - 1).saturating_mul(per_page);
    let result = store::list_library(&state.db, search, per_page, offset).await?;
    let stats = store::stats(&state.db).await?;
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
        stats,
    }))
}

/// `GET /api/v1/images/{id}`: one Image with all its Paths (Primary first).
/// Trashed Images are found too, with `trashed_at` set; an unknown id is 404.
pub async fn detail(
    State(state): State<AppState>,
    _owner: Owner,
    Path(id): Path<String>,
) -> Result<Json<ImageView>, ApiError> {
    let id = image_id(&id)?;
    let image = store::find_by_id(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(ImageView::new(image, &state.config.public_url)))
}

/// An Image id from the URL; anything but a number is an unknown Image.
fn image_id(id: &str) -> Result<i64, ApiError> {
    id.parse().map_err(|_| ApiError::NotFound)
}

#[derive(Deserialize)]
pub struct PathRequest {
    /// The Path without the leading `/`. Percent-encoded input is decoded.
    pub path: String,
}

fn path_request(body: Result<Json<PathRequest>, JsonRejection>) -> Result<String, ApiError> {
    body.map(|Json(req)| req.path)
        .map_err(|e| ApiError::BadRequest(e.body_text()))
}

/// `POST /api/v1/images/{id}/paths` with `{"path": "cat.png"}`: adds a Path
/// to the Image. Answers 201 with the updated Image; 400 `invalid_path` or
/// `reserved_path` for a Path that breaks the rules, 409 `path_taken` when
/// the Path already belongs to an Image.
pub async fn add_path(
    State(state): State<AppState>,
    _owner: Owner,
    Path(id): Path<String>,
    body: Result<Json<PathRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<ImageView>), ApiError> {
    let id = image_id(&id)?;
    let new_path = path::parse(&path_request(body)?)?;
    let now = state.now().unix_timestamp();
    let image = store::add_path(&state.db, id, &new_path, now).await?;
    Ok((
        StatusCode::CREATED,
        Json(ImageView::new(image, &state.config.public_url)),
    ))
}

/// `DELETE /api/v1/images/{id}/paths/{path}`: removes a Path (percent-
/// encoded in the URL like in a public link) from the Image. Answers the
/// updated Image; 409 `last_path` for the Image's only Path, 404 when the
/// Path is not one of the Image's.
pub async fn remove_path(
    State(state): State<AppState>,
    _owner: Owner,
    Path((id, path)): Path<(String, String)>,
) -> Result<Json<ImageView>, ApiError> {
    let id = image_id(&id)?;
    let image = store::remove_path(&state.db, id, &path).await?;
    Ok(Json(ImageView::new(image, &state.config.public_url)))
}

/// `PUT /api/v1/images/{id}/primary` with `{"path": "cat.png"}`: makes one
/// of the Image's Paths its Primary Path. Answers the updated Image; 404
/// when the Path is not one of the Image's.
pub async fn set_primary(
    State(state): State<AppState>,
    _owner: Owner,
    Path(id): Path<String>,
    body: Result<Json<PathRequest>, JsonRejection>,
) -> Result<Json<ImageView>, ApiError> {
    let id = image_id(&id)?;
    let path = path_request(body)?;
    let image = store::set_primary(&state.db, id, &path).await?;
    Ok(Json(ImageView::new(image, &state.config.public_url)))
}
