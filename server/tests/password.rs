//! Changing the Owner's password in the admin area
//! (`PUT /api/v1/session/password`).

mod common;

use axum::http::{Method, StatusCode};
use common::{TestApp, fixtures};
use img_bed::auth::{LOGIN_WINDOW, MAX_FAILED_LOGINS};
use serde_json::json;
use time::Duration;

const OLD: &str = "correct horse battery";
const NEW: &str = "staple tuning fork";
const URI: &str = "/api/v1/session/password";

#[tokio::test]
async fn the_new_password_works_for_login_and_the_old_one_does_not() {
    let app = TestApp::with_password(OLD).await;
    let session = app.login_ok(OLD, false).await;

    let res = app.change_password(&session, OLD, NEW).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());

    let res = app.login(OLD, false).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    assert_eq!(res.error_code(), "wrong_credentials");
    app.login_ok(NEW, false).await;

    // It is stored, not just held in memory.
    let app = app.restart().await;
    app.login_ok(NEW, false).await;
}

#[tokio::test]
async fn other_sessions_end_and_the_current_one_continues_with_its_expiry() {
    let app = TestApp::with_password(OLD).await;
    let other = app.login_ok(OLD, true).await;
    let another = app.login_ok(OLD, false).await;
    app.clock.advance(Duration::hours(1));
    let current = app.login_ok(OLD, false).await;
    let expires_at = app.get_as("/api/v1/session", &current).await.json()["expires_at"]
        .as_i64()
        .unwrap();

    let res = app.change_password(&current, OLD, NEW).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.json()["ended_sessions"], 2);
    assert!(res.session_set_cookie().is_none(), "the cookie is kept");

    assert!(!app.is_valid(&other).await);
    assert!(!app.is_valid(&another).await);
    let me = app.get_as("/api/v1/session", &current).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.json()["expires_at"].as_i64().unwrap(), expires_at);

    // The current session still works for everything, and still ends at the
    // time fixed at login.
    app.upload_ok(&current, "a.png", &fixtures::png(3, 2, "a"))
        .await;
    app.clock
        .advance(Duration::hours(24) - Duration::seconds(1));
    assert!(app.is_valid(&current).await);
    app.clock.advance(Duration::seconds(1));
    assert!(!app.is_valid(&current).await);
}

#[tokio::test]
async fn a_wrong_current_password_is_refused_and_changes_nothing() {
    let app = TestApp::with_password(OLD).await;
    let other = app.login_ok(OLD, false).await;
    let session = app.login_ok(OLD, false).await;

    let res = app.change_password(&session, "not the password", NEW).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert_eq!(res.error_code(), "wrong_current_password");

    // Not a logout: the session and every other one are still valid.
    assert!(app.is_valid(&session).await);
    assert!(app.is_valid(&other).await);
    assert_eq!(app.login(NEW, false).await.status, StatusCode::UNAUTHORIZED);
    app.login_ok(OLD, false).await;
}

#[tokio::test]
async fn wrong_current_passwords_count_toward_the_login_rate_limit() {
    let app = TestApp::with_password(OLD).await;
    let session = app.login_ok(OLD, false).await;

    for _ in 0..MAX_FAILED_LOGINS {
        let res = app.change_password(&session, "guess", NEW).await;
        assert_eq!(res.error_code(), "wrong_current_password");
    }

    // Further attempts are refused with 429, even with the right password,
    // both here and at login.
    let res = app.change_password(&session, OLD, NEW).await;
    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(res.error_code(), "too_many_attempts");
    let retry_after: i64 = res.header("retry-after").unwrap().parse().unwrap();
    assert_eq!(retry_after, LOGIN_WINDOW.whole_seconds());
    assert_eq!(
        app.login(OLD, false).await.status,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert!(app.is_valid(&session).await);

    // Once the window has passed, the change goes through.
    app.clock.advance(LOGIN_WINDOW);
    let res = app.change_password(&session, OLD, NEW).await;
    assert_eq!(res.status, StatusCode::OK);
    app.login_ok(NEW, false).await;
}

#[tokio::test]
async fn failed_logins_and_wrong_current_passwords_share_one_limit() {
    let app = TestApp::with_password(OLD).await;
    let session = app.login_ok(OLD, false).await;

    for _ in 0..MAX_FAILED_LOGINS - 1 {
        assert_eq!(
            app.login("guess", false).await.status,
            StatusCode::UNAUTHORIZED
        );
    }
    let res = app.change_password(&session, "guess", NEW).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);

    let res = app.change_password(&session, OLD, NEW).await;
    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn a_successful_change_forgives_earlier_failures() {
    let app = TestApp::with_password(OLD).await;
    let session = app.login_ok(OLD, false).await;
    for _ in 0..MAX_FAILED_LOGINS - 1 {
        app.login("guess", false).await;
    }
    let res = app.change_password(&session, OLD, NEW).await;
    assert_eq!(res.status, StatusCode::OK);

    for _ in 0..MAX_FAILED_LOGINS - 1 {
        app.login("guess", false).await;
    }
    app.login_ok(NEW, false).await;
}

#[tokio::test]
async fn a_too_short_new_password_is_refused() {
    let app = TestApp::with_password(OLD).await;
    let session = app.login_ok(OLD, false).await;

    let res = app.change_password(&session, OLD, "seven77").await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_eq!(res.error_code(), "password_too_short");
    let res = app.change_password(&session, OLD, "").await;
    assert_eq!(res.error_code(), "password_too_short");
    assert_eq!(
        app.login("seven77", false).await.status,
        StatusCode::UNAUTHORIZED
    );
    app.login_ok(OLD, false).await;

    // Characters are counted, not bytes: 8 characters is enough.
    let res = app.change_password(&session, OLD, "密码密码密码密码").await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    app.login_ok("密码密码密码密码", false).await;
}

#[tokio::test]
async fn bad_bodies_are_refused() {
    let app = TestApp::with_password(OLD).await;
    let session = app.login_ok(OLD, false).await;
    for body in [
        json!({}),
        json!({ "current_password": OLD }),
        json!({ "new_password": NEW }),
    ] {
        let res = app
            .request(Method::PUT, URI, Some(&session), Some(body))
            .await;
        assert!(res.status.is_client_error(), "{}", res.status);
        assert_ne!(res.status, StatusCode::OK);
    }
    app.login_ok(OLD, false).await;
}

#[tokio::test]
async fn changing_the_password_needs_a_session() {
    let app = TestApp::with_password(OLD).await;
    let body = json!({ "current_password": OLD, "new_password": NEW });

    let res = app.request(Method::PUT, URI, None, Some(body)).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    assert_eq!(res.error_code(), "unauthorized");
    app.login_ok(OLD, false).await;
}

#[tokio::test]
async fn an_api_token_cannot_change_the_password() {
    let app = TestApp::with_password(OLD).await;
    let session = app.login_ok(OLD, false).await;
    let (_, secret) = app.create_token_ok(&session, "PicGo").await;
    let body = json!({ "current_password": OLD, "new_password": NEW });

    let res = app
        .request_with_token(Method::PUT, URI, &secret, Some(body))
        .await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    assert_eq!(res.error_code(), "unauthorized");
    assert!(app.is_valid(&session).await);
    assert_eq!(app.login(NEW, false).await.status, StatusCode::UNAUTHORIZED);
    app.login_ok(OLD, false).await;
}

#[tokio::test]
async fn api_tokens_keep_working_after_a_change() {
    let app = TestApp::with_password(OLD).await;
    let session = app.login_ok(OLD, false).await;
    let (_, secret) = app.create_token_ok(&session, "PicGo").await;

    let res = app.change_password(&session, OLD, NEW).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.json()["ended_sessions"], 0);

    let res = app
        .upload_with_token(&secret, &[("a.png", &fixtures::png(2, 2, "b"))])
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
}

#[tokio::test]
async fn set_password_still_ends_every_session() {
    let app = TestApp::with_password(OLD).await;
    let a = app.login_ok(OLD, false).await;
    let b = app.login_ok(OLD, false).await;
    app.set_password(NEW).await;
    assert!(!app.is_valid(&a).await);
    assert!(!app.is_valid(&b).await);
}
