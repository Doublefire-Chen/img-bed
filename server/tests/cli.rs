//! The `img-bed` binary: `set-password` and `serve`.

mod common;

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use axum::http::StatusCode;
use common::TestApp;

fn stderr(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[tokio::test]
async fn set_password_sets_the_username_and_the_password() {
    let app = TestApp::new().await;
    let out = app.cli(&["set-password", "--username", "alice"], "first password\n");
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("\"alice\""));

    app.login_ok_as("alice", "first password", false).await;
    assert_eq!(
        app.login_as("alice", "other password", false).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.login_as("bob", "first password", false).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn the_first_set_password_needs_a_username() {
    let app = TestApp::new().await;
    let out = app.cli(&["set-password"], "first password\n");
    assert!(!out.status.success());
    assert!(stderr(&out).contains("--username"), "{}", stderr(&out));
    // Nothing was stored.
    assert_eq!(
        app.login_as("alice", "first password", false)
            .await
            .error_code(),
        "no_password"
    );
}

#[tokio::test]
async fn set_password_again_keeps_the_username_and_ends_every_session() {
    let app = TestApp::new().await;
    assert!(
        app.cli(&["set-password", "--username", "alice"], "first password\n")
            .status
            .success()
    );
    let day = app.login_ok_as("alice", "first password", false).await;
    let week = app.login_ok_as("alice", "first password", true).await;

    let out = app.cli(&["set-password"], "second password\n");
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("\"alice\""));

    assert!(!app.is_valid(&day).await);
    assert!(!app.is_valid(&week).await);
    assert_eq!(
        app.login_as("alice", "first password", false).await.status,
        StatusCode::UNAUTHORIZED
    );
    app.login_ok_as("alice", "second password", false).await;
}

#[tokio::test]
async fn set_password_with_a_new_username_replaces_it() {
    let app = TestApp::new().await;
    assert!(
        app.cli(&["set-password", "--username", "alice"], "first password\n")
            .status
            .success()
    );
    let out = app.cli(&["set-password", "--username", " Bob "], "first password\n");
    assert!(out.status.success(), "{}", stderr(&out));

    app.login_ok_as("bob", "first password", false).await;
    assert_eq!(
        app.login_as("alice", "first password", false)
            .await
            .error_code(),
        "wrong_credentials"
    );
    // Stored as entered (trimmed).
    let session = app.login_ok_as("BOB", "first password", false).await;
    assert_eq!(
        app.get_as("/api/v1/session", &session).await.json()["username"],
        "Bob"
    );
}

#[tokio::test]
async fn set_password_rejects_an_invalid_username() {
    let app = TestApp::new().await;
    let too_long = "a".repeat(65);
    for name in ["", "   ", "two words", "tab\there", too_long.as_str()] {
        let out = app.cli(&["set-password", "--username", name], "first password\n");
        assert!(!out.status.success(), "{name:?} was accepted");
        assert!(stderr(&out).contains("username"), "{}", stderr(&out));
    }
    let longest = "a".repeat(64);
    let out = app.cli(
        &["set-password", "--username", &longest],
        "first password\n",
    );
    assert!(out.status.success(), "{}", stderr(&out));
    app.login_ok_as(&longest, "first password", false).await;
}

#[tokio::test]
async fn set_password_rejects_a_short_password() {
    let app = TestApp::new().await;
    let out = app.cli(&["set-password", "--username", "alice"], "short\n");
    assert!(!out.status.success());
    assert!(stderr(&out).contains("at least"));
    assert_eq!(
        app.login_as("alice", "short", false).await.error_code(),
        "no_password"
    );
}

#[tokio::test]
async fn set_password_gives_an_old_database_its_username() {
    let app = TestApp::builder()
        .password("old password")
        .before_usernames()
        .build()
        .await;
    // Without a username it is refused, as on a first run.
    let out = app.cli(&["set-password"], "new password\n");
    assert!(!out.status.success());
    assert!(stderr(&out).contains("--username"), "{}", stderr(&out));

    let out = app.cli(&["set-password", "--username", "alice"], "new password\n");
    assert!(out.status.success(), "{}", stderr(&out));
    app.login_ok_as("alice", "new password", false).await;
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
