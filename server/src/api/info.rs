use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct Info {
    /// The server version; the admin frontend compares it with its own.
    pub version: &'static str,
}

/// `GET /api/v1/info` (public).
pub async fn info() -> Json<Info> {
    Json(Info {
        version: crate::VERSION,
    })
}
