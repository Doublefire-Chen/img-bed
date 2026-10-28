//! The Owner's username: logging in with it (`POST /api/v1/session`) and
//! changing it in the admin area (`PUT /api/v1/session/username`).

mod common;

use axum::http::StatusCode;
use common::TestApp;
use img_bed::auth::{LOGIN_WINDOW, MAX_FAILED_LOGINS};
use time::Duration;

const PASSWORD: &str = "correct horse battery";

async fn alice() -> TestApp {
    TestApp::builder()
        .username("Alice")
        .password(PASSWORD)
        .build()
        .await
}

// -- Login --------------------------------------------------------------

#[tokio::test]
async fn the_username_matches_in_any_case() {
    let app = alice().await;
    for name in ["Alice", "alice", "ALICE", "aLiCe", " alice "] {
        let session = app.login_ok_as(name, PASSWORD, false).await;
        let me = app.get_as("/api/v1/session", &session).await;
        assert_eq!(me.json()["username"], "Alice", "stored as entered");
    }
}

#[tokio::test]
async fn non_ascii_usernames_match_in_any_case() {
    let app = TestApp::builder()
        .username("Élodie")
        .password(PASSWORD)
        .build()
        .await;
    app.login_ok_as("élodie", PASSWORD, false).await;
    app.login_ok_as("ÉLODIE", PASSWORD, false).await;
    assert_eq!(
        app.login_as("elodie", PASSWORD, false).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn the_login_response_includes_the_username() {
    let app = alice().await;
    let res = app.login_as("alice", PASSWORD, false).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.json()["username"], "Alice");
}

#[tokio::test]
async fn a_wrong_username_and_a_wrong_password_get_identical_answers() {
    let app = alice().await;
    let wrong_name = app.login_as("bob", PASSWORD, false).await;
    let wrong_password = app.login_as("alice", "not the password", false).await;
    let both_wrong = app.login_as("bob", "not the password", false).await;

    for res in [&wrong_name, &wrong_password, &both_wrong] {
        assert_eq!(res.status, StatusCode::UNAUTHORIZED);
        assert_eq!(res.error_code(), "wrong_credentials");
        assert!(res.session_set_cookie().is_none());
    }
    assert_eq!(wrong_name.body, wrong_password.body);
    assert_eq!(wrong_name.body, both_wrong.body);
    let headers = |res: &common::TestResponse| {
        let mut h: Vec<_> = res
            .headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap().to_owned()))
            .collect();
        h.sort();
        h
    };
    assert_eq!(headers(&wrong_name), headers(&wrong_password));
}

#[tokio::test]
async fn wrong_usernames_and_wrong_passwords_share_the_failed_login_limit() {
    let app = alice().await;
    for i in 0..MAX_FAILED_LOGINS {
        let res = if i % 2 == 0 {
            app.login_as("mallory", PASSWORD, false).await
        } else {
            app.login_as("alice", "guess", false).await
        };
        assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    }
    let res = app.login_as("alice", PASSWORD, false).await;
    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS);

    app.clock.advance(LOGIN_WINDOW);
    app.login_ok_as("alice", PASSWORD, false).await;
}

#[tokio::test]
async fn only_wrong_usernames_also_hit_the_limit() {
    let app = alice().await;
    for _ in 0..MAX_FAILED_LOGINS {
        app.login_as("mallory", PASSWORD, false).await;
    }
    assert_eq!(
        app.login_as("alice", PASSWORD, false).await.status,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn a_login_without_a_username_is_refused() {
    let app = alice().await;
    let res = app
        .post_json(
            "/api/v1/session",
            serde_json::json!({ "password": PASSWORD, "remember_me": false }),
        )
        .await;
    assert!(res.status.is_client_error(), "{}", res.status);
    assert!(res.session_set_cookie().is_none());
}

#[tokio::test]
async fn a_database_without_a_username_says_to_run_set_password() {
    let app = TestApp::builder()
        .password(PASSWORD)
        .before_usernames()
        .build()
        .await;
    for name in ["", "owner", "anything"] {
        let res = app.login_as(name, PASSWORD, false).await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED);
        assert_eq!(res.error_code(), "no_username");
        assert!(
            res.json()["error"]["message"]
                .as_str()
                .unwrap()
                .contains("set-password --username"),
            "{}",
            res.text()
        );
        assert!(res.session_set_cookie().is_none());
    }
}

#[tokio::test]
async fn a_database_from_before_usernames_keeps_its_password() {
    let app = TestApp::builder()
        .password(PASSWORD)
        .before_usernames()
        .build()
        .await;
    let out = app.cli(
        &["set-password", "--username", "alice"],
        &format!("{PASSWORD}\n"),
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    app.login_ok_as("alice", PASSWORD, false).await;
}

#[tokio::test]
async fn an_unset_instance_still_says_no_password() {
    let app = TestApp::new().await;
    let res = app.login_as("alice", PASSWORD, false).await;
    assert_eq!(res.error_code(), "no_password");
    assert!(res.text().contains("set-password --username"));
}

// -- Changing it in Settings -------------------------------------------

#[tokio::test]
async fn the_new_username_works_for_login_and_the_old_one_does_not() {
    let app = alice().await;
    let session = app.login_ok_as("alice", PASSWORD, false).await;

    let res = app.change_username(&session, PASSWORD, "  Bob ").await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    assert_eq!(res.json()["username"], "Bob");

    let res = app.login_as("alice", PASSWORD, false).await;
    assert_eq!(res.error_code(), "wrong_credentials");
    app.login_ok_as("bob", PASSWORD, false).await;
    let me = app.get_as("/api/v1/session", &session).await;
    assert_eq!(me.json()["username"], "Bob");

    // Stored, not held in memory.
    let app = app.restart().await;
    app.login_ok_as("Bob", PASSWORD, false).await;
}

#[tokio::test]
async fn changing_the_username_ends_no_session() {
    let app = alice().await;
    let other = app.login_ok_as("alice", PASSWORD, true).await;
    let current = app.login_ok_as("alice", PASSWORD, false).await;

    let res = app.change_username(&current, PASSWORD, "bob").await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.session_set_cookie().is_none());
    assert!(app.is_valid(&other).await);
    assert!(app.is_valid(&current).await);
}

#[tokio::test]
async fn changing_only_the_case_is_allowed() {
    let app = alice().await;
    let session = app.login_ok_as("alice", PASSWORD, false).await;
    let res = app.change_username(&session, PASSWORD, "ALICE").await;
    assert_eq!(res.status, StatusCode::OK);
    let me = app.get_as("/api/v1/session", &session).await;
    assert_eq!(me.json()["username"], "ALICE");
}

#[tokio::test]
async fn a_wrong_current_password_changes_nothing() {
    let app = alice().await;
    let session = app.login_ok_as("alice", PASSWORD, false).await;

    let res = app
        .change_username(&session, "not the password", "bob")
        .await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert_eq!(res.error_code(), "wrong_current_password");

    assert!(app.is_valid(&session).await);
    app.login_ok_as("alice", PASSWORD, false).await;
    assert_eq!(
        app.login_as("bob", PASSWORD, false).await.error_code(),
        "wrong_credentials"
    );
}

#[tokio::test]
async fn wrong_current_passwords_count_toward_the_login_limit() {
    let app = alice().await;
    let session = app.login_ok_as("alice", PASSWORD, false).await;
    for _ in 0..MAX_FAILED_LOGINS {
        let res = app.change_username(&session, "guess", "bob").await;
        assert_eq!(res.status, StatusCode::FORBIDDEN);
    }
    let res = app.change_username(&session, PASSWORD, "bob").await;
    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(res.header("retry-after").is_some());
    assert_eq!(
        app.login_as("alice", PASSWORD, false).await.status,
        StatusCode::TOO_MANY_REQUESTS
    );

    app.clock.advance(LOGIN_WINDOW + Duration::seconds(1));
    let res = app.change_username(&session, PASSWORD, "bob").await;
    assert_eq!(res.status, StatusCode::OK);
}

#[tokio::test]
async fn invalid_usernames_are_refused_without_costing_an_attempt() {
    let app = alice().await;
    let session = app.login_ok_as("alice", PASSWORD, false).await;
    let too_long = "x".repeat(65);
    let invalid = [
        "",
        "   ",
        "two words",
        "new\nline",
        "nul\u{0}",
        too_long.as_str(),
    ];
    for name in invalid {
        let res = app.change_username(&session, "wrong password", name).await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "{name:?}");
        assert_eq!(res.error_code(), "invalid_username");
    }
    assert!(invalid.len() >= MAX_FAILED_LOGINS);
    app.login_ok_as("alice", PASSWORD, false).await;

    let longest = "ü".repeat(64);
    let res = app.change_username(&session, PASSWORD, &longest).await;
    assert_eq!(res.status, StatusCode::OK, "64 characters, not bytes");
    app.login_ok_as(&longest, PASSWORD, false).await;
}

#[tokio::test]
async fn changing_the_password_keeps_the_username() {
    let app = alice().await;
    let session = app.login_ok_as("alice", PASSWORD, false).await;
    let res = app
        .change_password(&session, PASSWORD, "a brand new password")
        .await;
    assert_eq!(res.status, StatusCode::OK);
    app.login_ok_as("alice", "a brand new password", false)
        .await;
}
