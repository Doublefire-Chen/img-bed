//! The `img-bed` binary: `set-password` and `serve`.

mod common;

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use axum::http::StatusCode;
use common::TestApp;

#[tokio::test]
async fn set_password_sets_the_password() {
    let app = TestApp::new().await;
    let out = app.cli(&["set-password"], "first password\n");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    app.login_ok("first password", false).await;
    assert_eq!(
        app.login("other password", false).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn set_password_again_changes_it_and_ends_every_session() {
    let app = TestApp::new().await;
    assert!(
        app.cli(&["set-password"], "first password\n")
            .status
            .success()
    );
    let day = app.login_ok("first password", false).await;
    let week = app.login_ok("first password", true).await;

    let out = app.cli(&["set-password"], "second password\n");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(!app.is_valid(&day).await);
    assert!(!app.is_valid(&week).await);
    assert_eq!(
        app.login("first password", false).await.status,
        StatusCode::UNAUTHORIZED
    );
    app.login_ok("second password", false).await;
}

#[tokio::test]
async fn set_password_rejects_a_short_password() {
    let app = TestApp::new().await;
    let out = app.cli(&["set-password"], "short\n");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("at least"));
    assert_eq!(app.login("short", false).await.error_code(), "no_password");
}

#[test]
fn missing_config_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let out = common::run_cli(&dir.path().join("nope.toml"), &["set-password"], "x\n");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("config"));
}

#[test]
fn serve_starts_from_config_and_creates_the_database() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("img-bed.toml");
    std::fs::write(
        &config,
        "listen = \"127.0.0.1:0\"\ndata_dir = \"state/data\"\nfrontend_dir = \"web\"\n",
    )
    .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_img-bed"))
        .args(["--config", config.to_str().unwrap(), "serve"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    child.kill().unwrap();
    child.wait().unwrap();

    assert!(line.contains("listening on http://127.0.0.1:"), "{line}");
    assert!(
        dir.path()
            .join("state/data")
            .join(img_bed::db::DB_FILE)
            .is_file()
    );
}
