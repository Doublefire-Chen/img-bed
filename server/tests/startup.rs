//! Server and data: startup from config, data directory, Migrations, info.

mod common;

use axum::http::StatusCode;
use common::TestApp;

#[tokio::test]
async fn startup_creates_data_directory_and_database() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("nested/does/not/exist");
    let db = img_bed::db::open(&data_dir).await.unwrap();
    db.close().await;
    assert!(data_dir.join(img_bed::db::DB_FILE).is_file());
}

#[tokio::test]
async fn migrations_are_idempotent_and_state_survives_a_restart() {
    let app = TestApp::with_password("correct horse").await;
    let session = app.login_ok("correct horse", false).await;

    // Opening the same data directory again re-runs the migrator.
    let restarted = app.restart().await;
    assert!(restarted.is_valid(&session).await);
    restarted.login_ok("correct horse", false).await;
}

#[tokio::test]
async fn info_reports_server_version_without_a_session() {
    let app = TestApp::new().await;
    let res = app.get("/api/v1/info").await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.json()["version"], env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn paths_outside_reserved_prefixes_are_404_for_now() {
    let app = TestApp::new().await;
    assert_eq!(app.get("/k3J9xQ.png").await.status, StatusCode::NOT_FOUND);
}
