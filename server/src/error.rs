//! API errors. Every error response has the JSON shape
//! `{"error": {"code": "...", "message": "..."}}`.

use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("not logged in")]
    Unauthorized,
    #[error("wrong password")]
    WrongPassword,
    #[error("no password has been set; run `img-bed set-password` on the server")]
    NoPassword,
    #[error("too many failed logins; try again in {retry_after_secs} seconds")]
    TooManyAttempts { retry_after_secs: u64 },
    #[error("not found")]
    NotFound,
    #[error("method not allowed")]
    MethodNotAllowed,
    #[error("{0}")]
    BadRequest(String),
    #[error("unsupported file type; accepted types are JPEG, PNG, GIF, WebP, AVIF and SVG")]
    UnsupportedType,
    #[error("file is larger than the upload limit of {limit_mb} MB")]
    TooLarge { limit_mb: u64 },
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

impl ApiError {
    pub fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::WrongPassword => (StatusCode::UNAUTHORIZED, "wrong_password"),
            Self::NoPassword => (StatusCode::UNAUTHORIZED, "no_password"),
            Self::TooManyAttempts { .. } => (StatusCode::TOO_MANY_REQUESTS, "too_many_attempts"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::MethodNotAllowed => (StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed"),
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            Self::UnsupportedType => (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported_type"),
            Self::TooLarge { .. } => (StatusCode::PAYLOAD_TOO_LARGE, "too_large"),
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        Self::Internal(e.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let Self::Internal(e) = &self {
            tracing::error!("internal error: {e:#}");
        }
        let (status, code) = self.status_and_code();
        let body = json!({ "error": { "code": code, "message": self.to_string() } });
        let mut response = (status, Json(body)).into_response();
        if let Self::TooManyAttempts { retry_after_secs } = self {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from(retry_after_secs));
        }
        response
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
