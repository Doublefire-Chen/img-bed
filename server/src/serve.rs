//! Serving Images to Visitors at their Paths: the fallback for every request
//! outside the Reserved Prefixes.
//!
//! Each Path serves the file directly (no redirect). The response headers
//! below are load-bearing (ADR 0001): images share the admin area's origin,
//! so the sandbox CSP and `nosniff` keep an uploaded SVG from running script
//! against the Owner's session.

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use percent_encoding::percent_decode_str;
use tokio_util::io::ReaderStream;

use crate::state::AppState;
use crate::store;

pub const CONTENT_SECURITY_POLICY: &str = "sandbox; default-src 'none'; style-src 'unsafe-inline'";
pub const CACHE_CONTROL: &str = "public, max-age=3600";

pub async fn serve_image(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    if method != Method::GET && method != Method::HEAD {
        return (
            StatusCode::METHOD_NOT_ALLOWED,
            [(header::ALLOW, "GET, HEAD")],
        )
            .into_response();
    }
    let Ok(path) = percent_decode_str(uri.path().trim_start_matches('/')).decode_utf8() else {
        return not_found();
    };
    if path.is_empty() {
        return not_found();
    }
    let resolved = match store::resolve(&state.db, &path).await {
        Ok(Some(r)) => r,
        Ok(None) => return not_found(),
        Err(e) => return e.into_response(),
    };
    file_response(&state, &resolved, &method, &headers, CACHE_CONTROL).await
}

/// The response carrying an Image's file, with the load-bearing headers.
/// Also used for the Owner's previews (`GET /api/v1/images/{id}/file`).
pub async fn file_response(
    state: &AppState,
    resolved: &store::Resolved,
    method: &Method,
    headers: &HeaderMap,
    cache_control: &'static str,
) -> Response {
    let etag = format!("\"{}\"", resolved.sha256);
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CONTENT_SECURITY_POLICY),
    );
    response_headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response_headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control),
    );
    response_headers.insert(
        header::ETAG,
        HeaderValue::from_str(&etag).expect("hex is a valid header value"),
    );

    if if_none_match(headers, &resolved.sha256) {
        return (StatusCode::NOT_MODIFIED, response_headers).into_response();
    }

    response_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&resolved.mime).expect("stored MIME type is a valid header"),
    );
    response_headers.insert(header::CONTENT_LENGTH, HeaderValue::from(resolved.size));

    if *method == Method::HEAD {
        return (StatusCode::OK, response_headers).into_response();
    }
    let file_path = store::file_path(&state.config.data_dir, &resolved.sha256);
    let file = match tokio::fs::File::open(&file_path).await {
        Ok(f) => f,
        Err(e) => {
            tracing::error!("Image file missing at {}: {e}", file_path.display());
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let body = Body::from_stream(ReaderStream::new(file));
    (StatusCode::OK, response_headers, body).into_response()
}

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        [(header::X_CONTENT_TYPE_OPTIONS, "nosniff")],
        "Not Found",
    )
        .into_response()
}

/// Whether `If-None-Match` lists this Image's ETag (strong or weak) or `*`.
fn if_none_match(headers: &HeaderMap, sha256: &str) -> bool {
    headers
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(str::trim)
        .any(|tag| {
            tag == "*"
                || tag
                    .strip_prefix("W/")
                    .unwrap_or(tag)
                    .strip_prefix('"')
                    .and_then(|t| t.strip_suffix('"'))
                    == Some(sha256)
        })
}

/// Characters left as-is in a Path segment of a public URL; everything else
/// (including non-ASCII letters from `{name}`) is percent-encoded.
const SEGMENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// The full public URL of a Path: `public_url` + `/` + the encoded Path.
pub fn url_for(public_url: &str, path: &str) -> String {
    let encoded: Vec<String> = path
        .split('/')
        .map(|segment| percent_encoding::utf8_percent_encode(segment, SEGMENT).to_string())
        .collect();
    format!("{public_url}/{}", encoded.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_encode_each_segment() {
        let base = "https://img.aka.cy";
        assert_eq!(url_for(base, "k3J9xQ.png"), "https://img.aka.cy/k3J9xQ.png");
        assert_eq!(
            url_for(base, "2026/03/07/a-b_c~d.jpg"),
            "https://img.aka.cy/2026/03/07/a-b_c~d.jpg"
        );
        assert_eq!(
            url_for(base, "截图-ab.png"),
            "https://img.aka.cy/%E6%88%AA%E5%9B%BE-ab.png"
        );
        assert_eq!(url_for(base, "a b?.png"), "https://img.aka.cy/a%20b%3F.png");
    }
}
