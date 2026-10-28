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
    /// An `Authorization: Bearer` API Token that is unknown or revoked.
    #[error("the API Token is invalid or has been revoked")]
    InvalidToken,
    /// A login with the wrong username or the wrong password; the two are
    /// deliberately indistinguishable.
    #[error("wrong username or password")]
    WrongCredentials,
    /// The current password given when changing it is wrong. Not 401: the
    /// session is still valid.
    #[error("the current password is wrong")]
    WrongCurrentPassword,
    /// A new password shorter than [`crate::auth::MIN_PASSWORD_LEN`].
    #[error(
        "the new password must be at least {} characters",
        crate::auth::MIN_PASSWORD_LEN
    )]
    PasswordTooShort,
    #[error("no password has been set; run `img-bed set-password --username <name>` on the server")]
    NoPassword,
    /// A password is set but no username (a database from before usernames).
    #[error("no username has been set; run `img-bed set-password --username <name>` on the server")]
    NoUsername,
    /// A username that breaks the rules (see [`crate::auth::clean_username`]).
    #[error("{0}")]
    InvalidUsername(&'static str),
    #[error("too many failed password attempts; try again in {retry_after_secs} seconds")]
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
    /// A Path the Owner typed in breaks the Path rules.
    #[error("{0}")]
    InvalidPath(crate::path::PathError),
    /// A Path the Owner typed in starts with a Reserved Prefix.
    #[error("/{0} starts with a Reserved Prefix (admin/ or api/) and can never be a Path")]
    ReservedPath(String),
    /// The Path already belongs to an Image (this one, another, or one in
    /// the Trash).
    #[error("{}", path_taken_message(.path, *.held_by))]
    PathTaken {
        path: String,
        held_by: crate::store::PathHolder,
    },
    /// Removing the Path would leave the Image without one.
    #[error("/{0} is the Image's only Path and cannot be removed")]
    LastPath(String),
    /// The Paths of a trashed Image can't be changed.
    #[error("the Image is in the Trash; Restore it to change its Paths")]
    ImageInTrash,
    /// Permanent Delete is only for Images in the Trash.
    #[error("{} not in the Trash; move {} to the Trash first", if *.0 == 1 { "1 Image is" } else { "some Images are" }, if *.0 == 1 { "it" } else { "them" })]
    NotInTrash(usize),
    /// An active API Token already has this name.
    #[error("an active API Token is already named {0:?}")]
    TokenNameTaken(String),
    /// A Path Template the Owner tried to save or preview is invalid.
    #[error("{0}")]
    InvalidTemplate(#[from] crate::path_template::TemplateError),
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

impl ApiError {
    pub fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::InvalidToken => (StatusCode::UNAUTHORIZED, "invalid_token"),
            Self::WrongCredentials => (StatusCode::UNAUTHORIZED, "wrong_credentials"),
            Self::WrongCurrentPassword => (StatusCode::FORBIDDEN, "wrong_current_password"),
            Self::PasswordTooShort => (StatusCode::BAD_REQUEST, "password_too_short"),
            Self::NoPassword => (StatusCode::UNAUTHORIZED, "no_password"),
            Self::NoUsername => (StatusCode::UNAUTHORIZED, "no_username"),
            Self::InvalidUsername(_) => (StatusCode::BAD_REQUEST, "invalid_username"),
            Self::TooManyAttempts { .. } => (StatusCode::TOO_MANY_REQUESTS, "too_many_attempts"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::MethodNotAllowed => (StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed"),
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            Self::UnsupportedType => (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported_type"),
            Self::TooLarge { .. } => (StatusCode::PAYLOAD_TOO_LARGE, "too_large"),
            Self::InvalidPath(_) => (StatusCode::BAD_REQUEST, "invalid_path"),
            Self::ReservedPath(_) => (StatusCode::BAD_REQUEST, "reserved_path"),
            Self::PathTaken {
                held_by: crate::store::PathHolder::ImageInTrash,
                ..
            } => (StatusCode::CONFLICT, "path_in_trash"),
            Self::PathTaken { .. } => (StatusCode::CONFLICT, "path_taken"),
            Self::LastPath(_) => (StatusCode::CONFLICT, "last_path"),
            Self::ImageInTrash => (StatusCode::CONFLICT, "image_in_trash"),
            Self::NotInTrash(_) => (StatusCode::CONFLICT, "not_in_trash"),
            Self::TokenNameTaken(_) => (StatusCode::CONFLICT, "token_name_taken"),
            Self::InvalidTemplate(e) => {
                use crate::path_template::TemplateError;
                let code = match e {
                    TemplateError::NoUniquePart => "template_not_unique",
                    TemplateError::UnknownVariable(_) => "unknown_variable",
                    _ => "invalid_template",
                };
                (StatusCode::BAD_REQUEST, code)
            }
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    }
}

fn path_taken_message(path: &str, held_by: crate::store::PathHolder) -> String {
    use crate::store::PathHolder;
    match held_by {
        PathHolder::ThisImage => format!("/{path} is already a Path of this Image"),
        PathHolder::AnotherImage => format!("/{path} is already taken by another Image"),
        PathHolder::ImageInTrash => format!(
            "/{path} is held by an Image in Trash; it is free once that Image is Permanently Deleted"
        ),
    }
}

impl From<crate::path::PathError> for ApiError {
    fn from(e: crate::path::PathError) -> Self {
        match e {
            crate::path::PathError::Reserved(path) => Self::ReservedPath(path),
            e => Self::InvalidPath(e),
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
        if matches!(self, Self::InvalidToken) {
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static("Bearer error=\"invalid_token\""),
            );
        }
        if let Self::TooManyAttempts { retry_after_secs } = self {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from(retry_after_secs));
        }
        response
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
