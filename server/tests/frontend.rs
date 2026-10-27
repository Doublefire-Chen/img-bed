//! Serving the admin frontend from `frontend_dir` under `admin/`.

mod common;

use axum::http::StatusCode;
use common::{APP_JS, INDEX_HTML, TestApp};

#[tokio::test]
async fn admin_root_serves_index() {
    let app = TestApp::new().await;
    let res = app.get("/admin/").await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.text(), INDEX_HTML);
    assert_eq!(res.header("cache-control"), Some("no-cache"));
}

#[tokio::test]
async fn admin_without_trailing_slash_redirects() {
    let app = TestApp::new().await;
    let res = app.get("/admin").await;
    assert_eq!(res.status, StatusCode::PERMANENT_REDIRECT);
    assert_eq!(res.header("location"), Some("/admin/"));
}

#[tokio::test]
async fn admin_serves_files_from_frontend_dir() {
    let app = TestApp::new().await;
    let res = app.get("/admin/assets/app.js").await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.text(), APP_JS);
    assert!(res.header("content-type").unwrap().contains("javascript"));
    assert!(res.header("cache-control").unwrap().contains("immutable"));
}

#[tokio::test]
async fn deep_links_fall_back_to_index() {
    let app = TestApp::new().await;
    for uri in [
        "/admin/login",
        "/admin/library/some/deep/link",
        "/admin/trash?page=2",
    ] {
        let res = app.get(uri).await;
        assert_eq!(res.status, StatusCode::OK, "{uri}");
        assert_eq!(res.text(), INDEX_HTML, "{uri}");
    }
}

#[tokio::test]
async fn frontend_is_read_from_disk_not_embedded() {
    let app = TestApp::new().await;
    std::fs::write(app.config.frontend_dir.join("index.html"), "replaced").unwrap();
    assert_eq!(app.get("/admin/").await.text(), "replaced");
}
