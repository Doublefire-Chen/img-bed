//! `api/v1/images`: uploading Images.

use axum::Json;
use axum::extract::State;
use axum::extract::multipart::{Multipart, MultipartError, MultipartRejection};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::json;

use crate::auth::Owner;
use crate::error::ApiError;
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
        /// The bytes matched an existing Image; it is returned unchanged.
        duplicate: bool,
        image: ImageView,
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
                results.push(UploadResult::Stored {
                    filename,
                    duplicate: ingested.duplicate,
                    image: ImageView::new(ingested.image, &state.config.public_url),
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
