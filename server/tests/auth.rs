//! Owner login: cookies, expiry, rate limit, logout and protection.

mod common;

use axum::http::{Method, StatusCode};
use common::{Session, TestApp};
use time::Duration;

const PASSWORD: &str = "correct horse battery";

#[tokio::test]
async fn login_sets_a_secure_session_cookie() {
    let app = TestApp::with_password(PASSWORD).await;
    let res = app.login(PASSWORD, false).await;
    assert_eq!(res.status, StatusCode::OK);

    let attrs = res.session_cookie_attributes();
    for required in ["httponly", "secure", "samesite=strict", "path=/"] {
        assert!(
            attrs.iter().any(|a| a == required),
            "missing {required}: {attrs:?}"
        );
    }
    assert!(attrs.contains(&"max-age=86400".to_owned()), "{attrs:?}");

    let expires_at = res.json()["expires_at"].as_i64().unwrap();
    assert_eq!(expires_at, common::START + 24 * 3600);

    let session = res.session().unwrap();
    let me = app.get_as("/api/v1/session", &session).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.json()["expires_at"].as_i64().unwrap(), expires_at);
}

#[tokio::test]
async fn wrong_password_is_rejected_without_a_cookie() {
    let app = TestApp::with_password(PASSWORD).await;
    let res = app.login("wrong password", false).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    assert_eq!(res.error_code(), "wrong_credentials");
    assert!(res.session_set_cookie().is_none());
}

#[tokio::test]
async fn login_before_any_password_is_set_is_rejected() {
    let app = TestApp::new().await;
    let res = app.login("anything at all", false).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    assert_eq!(res.error_code(), "no_password");
    assert!(res.session_set_cookie().is_none());
}

#[tokio::test]
async fn session_ends_24_hours_after_login() {
    let app = TestApp::with_password(PASSWORD).await;
    let session = app.login_ok(PASSWORD, false).await;

    app.clock
        .advance(Duration::hours(24) - Duration::seconds(1));
    assert!(app.is_valid(&session).await);

    app.clock.advance(Duration::seconds(1));
    assert!(!app.is_valid(&session).await);
}

#[tokio::test]
async fn remember_me_session_ends_7_days_after_login() {
    let app = TestApp::with_password(PASSWORD).await;
    let res = app.login(PASSWORD, true).await;
    assert!(
        res.session_cookie_attributes()
            .contains(&"max-age=604800".to_owned())
    );
    let session = res.session().unwrap();

    app.clock.advance(Duration::hours(25));
    assert!(app.is_valid(&session).await);

    app.clock
        .advance(Duration::days(7) - Duration::hours(25) - Duration::seconds(1));
    assert!(app.is_valid(&session).await);

    app.clock.advance(Duration::seconds(1));
    assert!(!app.is_valid(&session).await);
}

#[tokio::test]
async fn activity_does_not_extend_a_session() {
    let app = TestApp::with_password(PASSWORD).await;
    let session = app.login_ok(PASSWORD, false).await;
    for _ in 0..23 {
        app.clock.advance(Duration::hours(1));
        assert!(app.is_valid(&session).await);
    }
    app.clock.advance(Duration::hours(1));
    assert!(!app.is_valid(&session).await);
}

#[tokio::test]
async fn repeated_failed_logins_are_rate_limited() {
    let app = TestApp::with_password(PASSWORD).await;
    for _ in 0..img_bed::auth::MAX_FAILED_LOGINS {
        assert_eq!(
            app.login("guess", false).await.status,
            StatusCode::UNAUTHORIZED
        );
    }

    // Further attempts are refused, even with the right password.
    let res = app.login(PASSWORD, false).await;
    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(res.error_code(), "too_many_attempts");
    assert!(res.session_set_cookie().is_none());
    let retry_after: i64 = res.header("retry-after").unwrap().parse().unwrap();
    assert_eq!(retry_after, img_bed::auth::LOGIN_WINDOW.whole_seconds());

    app.clock.advance(Duration::minutes(5));
    assert_eq!(
        app.login(PASSWORD, false).await.status,
        StatusCode::TOO_MANY_REQUESTS
    );

    // Once the window has passed, the Owner can log in again.
    app.clock
        .advance(img_bed::auth::LOGIN_WINDOW - Duration::minutes(5));
    app.login_ok(PASSWORD, false).await;
}

#[tokio::test]
async fn a_successful_login_forgives_earlier_failures() {
    let app = TestApp::with_password(PASSWORD).await;
    for _ in 0..img_bed::auth::MAX_FAILED_LOGINS - 1 {
        app.login("typo", false).await;
    }
    app.login_ok(PASSWORD, false).await;
    for _ in 0..img_bed::auth::MAX_FAILED_LOGINS - 1 {
        app.login("typo", false).await;
    }
    app.login_ok(PASSWORD, false).await;
}

#[tokio::test]
async fn logout_ends_the_session() {
    let app = TestApp::with_password(PASSWORD).await;
    let session = app.login_ok(PASSWORD, false).await;
    let other = app.login_ok(PASSWORD, false).await;

    let res = app.delete_as("/api/v1/session", &session).await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    assert!(
        res.session_cookie_attributes()
            .contains(&"max-age=0".to_owned())
    );
    assert!(res.session().is_none(), "cookie value is cleared");

    assert!(!app.is_valid(&session).await);
    assert_eq!(
        app.delete_as("/api/v1/session", &session).await.status,
        StatusCode::UNAUTHORIZED
    );
    // Only that session ends.
    assert!(app.is_valid(&other).await);
}

#[tokio::test]
async fn protected_endpoints_return_401_without_a_session() {
    let app = TestApp::with_password(PASSWORD).await;
    let forged = Session {
        token: "not-a-real-session".into(),
    };
    for (method, uri) in [
        (Method::GET, "/api/v1/session"),
        (Method::DELETE, "/api/v1/session"),
        (Method::PUT, "/api/v1/session/password"),
        (Method::PUT, "/api/v1/session/username"),
        (Method::GET, "/api/v1/images"),
        (Method::POST, "/api/v1/images"),
        (Method::POST, "/api/v1/anything/else"),
        (Method::GET, "/api/v2/info"),
    ] {
        for session in [None, Some(&forged)] {
            let res = app.request(method.clone(), uri, session, None).await;
            assert_eq!(
                res.status,
                StatusCode::UNAUTHORIZED,
                "{method} {uri} {session:?}"
            );
            assert_eq!(res.error_code(), "unauthorized");
        }
    }
}

#[tokio::test]
async fn unknown_api_paths_are_404_with_a_session() {
    let app = TestApp::with_password(PASSWORD).await;
    let session = app.login_ok(PASSWORD, false).await;
    let res = app.get_as("/api/v1/nope", &session).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}
