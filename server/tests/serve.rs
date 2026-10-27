//! Serving Images to Visitors at their Paths.

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use common::fixtures;
use common::{PUBLIC_URL, TestApp};

const PASSWORD: &str = "correct horse battery";

/// An app with one uploaded Image; returns the app, the Image JSON and its bytes.
async fn app_with_image(bytes: Vec<u8>) -> (TestApp, serde_json::Value, Vec<u8>) {
    let app = TestApp::with_password(PASSWORD).await;
    let session = app.login_ok(PASSWORD, false).await;
    let image = app.upload_ok(&session, "pic", &bytes).await;
    (app, image, bytes)
}

fn uri(image: &serde_json::Value) -> String {
    format!("/{}", image["path"].as_str().unwrap())
}

#[tokio::test]
async fn visitors_get_the_image_directly_with_protective_headers() {
    let (app, image, bytes) = app_with_image(fixtures::png(2, 2, "")).await;
    let res = app.get(&uri(&image)).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(&res.body[..], &bytes[..]);
    assert_eq!(res.header("content-type"), Some("image/png"));
    assert_eq!(
        res.header("content-security-policy"),
        Some("sandbox; default-src 'none'; style-src 'unsafe-inline'")
    );
    assert_eq!(res.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(res.header("cache-control"), Some("public, max-age=3600"));
    let etag = format!("\"{}\"", image["sha256"].as_str().unwrap());
    assert_eq!(res.header("etag"), Some(etag.as_str()));
    assert_eq!(
        res.header("content-length"),
        Some(bytes.len().to_string().as_str())
    );
    assert!(res.header("location").is_none());
}

#[tokio::test]
async fn svg_is_served_as_svg_inside_the_sandbox() {
    let (app, image, bytes) = app_with_image(fixtures::svg("x")).await;
    let res = app.get(&uri(&image)).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(&res.body[..], &bytes[..]);
    assert_eq!(res.header("content-type"), Some("image/svg+xml"));
    assert!(
        res.header("content-security-policy")
            .unwrap()
            .starts_with("sandbox;")
    );
    assert_eq!(res.header("x-content-type-options"), Some("nosniff"));
}

#[tokio::test]
async fn a_matching_if_none_match_gets_304() {
    let (app, image, _) = app_with_image(fixtures::gif(1, 1, "")).await;
    let sha = image["sha256"].as_str().unwrap();
    for value in [
        format!("\"{sha}\""),
        format!("W/\"{sha}\""),
        format!("\"other\", \"{sha}\""),
        "*".to_owned(),
    ] {
        let res = app
            .get_with(&uri(&image), &[("if-none-match", &value)])
            .await;
        assert_eq!(res.status, StatusCode::NOT_MODIFIED, "{value}");
        assert!(res.body.is_empty());
        assert_eq!(res.header("etag"), Some(format!("\"{sha}\"").as_str()));
        assert_eq!(res.header("cache-control"), Some("public, max-age=3600"));
    }

    let res = app
        .get_with(&uri(&image), &[("if-none-match", "\"stale\"")])
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(!res.body.is_empty());
}

#[tokio::test]
async fn unknown_paths_are_404() {
    let (app, image, _) = app_with_image(fixtures::png(1, 1, "")).await;
    let path = image["path"].as_str().unwrap();
    let flipped: String = path
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() {
                c.to_ascii_uppercase()
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect();
    for unknown in [
        "/nope.png".to_owned(),
        "/".to_owned(),
        format!("/{flipped}"), // Paths are case-sensitive
        format!("/{path}/"),
        format!("/x/{path}"),
    ] {
        let res = app.get(&unknown).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "{unknown}");
    }
}

#[tokio::test]
async fn head_returns_headers_without_a_body() {
    let (app, image, bytes) = app_with_image(fixtures::png(1, 1, "")).await;
    let res = app
        .send(
            Request::builder()
                .method(Method::HEAD)
                .uri(uri(&image))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.body.is_empty());
    assert_eq!(res.header("content-type"), Some("image/png"));
    assert_eq!(
        res.header("content-length"),
        Some(bytes.len().to_string().as_str())
    );
}

#[tokio::test]
async fn other_methods_are_not_allowed() {
    let (app, image, _) = app_with_image(fixtures::png(1, 1, "")).await;
    let res = app.request(Method::DELETE, &uri(&image), None, None).await;
    assert_eq!(res.status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn non_ascii_paths_are_served_at_their_percent_encoded_url() {
    let app = TestApp::builder()
        .password(PASSWORD)
        .path_template("{name}-{hash:4}.{ext}")
        .build()
        .await;
    let session = app.login_ok(PASSWORD, false).await;
    let bytes = fixtures::png(1, 1, "");
    let image = app.upload_ok(&session, "截图 1.png", &bytes).await;
    let path = image["path"].as_str().unwrap();
    assert!(path.starts_with("截图-1-"), "{path}");
    let url = image["url"].as_str().unwrap();
    let encoded = url.strip_prefix(PUBLIC_URL).unwrap();
    assert!(encoded.starts_with("/%E6%88%AA%E5%9B%BE-1-"), "{url}");
    let res = app.get(encoded).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(&res.body[..], &bytes[..]);
}

#[tokio::test]
async fn reserved_prefixes_are_not_image_paths() {
    let app = TestApp::new().await;
    // The admin area and the API keep answering as before.
    assert_eq!(app.get("/admin/").await.text(), common::INDEX_HTML);
    assert_eq!(
        app.get("/api/v1/nope").await.status,
        StatusCode::UNAUTHORIZED
    );
}
