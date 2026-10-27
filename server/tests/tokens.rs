//! API Tokens: creating, listing and revoking them, uploading with one, and
//! the token being refused everywhere else.

mod common;

use axum::http::{Method, StatusCode};
use common::fixtures::{jpeg, png};
use common::{PUBLIC_URL, START, Session, TestApp, TestResponse};
use serde_json::{Value, json};
use time::Duration;

const PASSWORD: &str = "correct horse battery";

async fn app_and_session() -> (TestApp, Session) {
    let app = TestApp::with_password(PASSWORD).await;
    let session = app.login_ok(PASSWORD, false).await;
    (app, session)
}

async fn create(app: &TestApp, session: Option<&Session>, body: Value) -> TestResponse {
    app.request(Method::POST, "/api/v1/tokens", session, Some(body))
        .await
}

async fn list(app: &TestApp, session: &Session) -> Value {
    let res = app.get_as("/api/v1/tokens", session).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    res.json()
}

/// The listed token with this id.
async fn listed(app: &TestApp, session: &Session, id: &Value) -> Value {
    list(app, session).await["tokens"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == *id)
        .cloned()
        .expect("token is listed")
}

async fn revoke(app: &TestApp, session: &Session, id: &Value) -> TestResponse {
    app.delete_as(&format!("/api/v1/tokens/{id}"), session)
        .await
}

fn assert_error(res: &TestResponse, status: StatusCode, code: &str) {
    assert_eq!(res.status, status, "{}", res.text());
    assert_eq!(res.error_code(), code, "{}", res.text());
}

fn assert_token_refused(res: &TestResponse) {
    assert_error(res, StatusCode::UNAUTHORIZED, "invalid_token");
    assert_eq!(
        res.header("www-authenticate"),
        Some("Bearer error=\"invalid_token\"")
    );
}

#[tokio::test]
async fn creating_a_token_returns_its_secret_once_and_listing_never_shows_it() {
    let (app, session) = app_and_session().await;
    let (token, secret) = app.create_token_ok(&session, "  PicGo laptop ").await;

    assert_eq!(token["name"], "PicGo laptop", "the name is trimmed");
    assert_eq!(token["created_at"], START);
    assert!(token["last_used_at"].is_null());
    assert!(token["revoked_at"].is_null());
    assert!(token["id"].is_i64());
    assert!(secret.starts_with("imgbed_"), "{secret}");
    // 32 random bytes, base64url without padding.
    assert_eq!(secret.len(), "imgbed_".len() + 43, "{secret}");
    let random = &secret["imgbed_".len()..];
    assert!(
        random
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    );

    let (_, other) = app.create_token_ok(&session, "ShareX").await;
    assert_ne!(secret, other, "every token gets its own secret");

    let res = app.get_as("/api/v1/tokens", &session).await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(!res.text().contains(&secret), "{}", res.text());
    assert!(!res.text().contains(&other));
    assert!(!res.text().contains("secret"), "{}", res.text());
    let tokens = res.json()["tokens"].as_array().unwrap().clone();
    let names: Vec<_> = tokens.iter().map(|t| t["name"].clone()).collect();
    assert_eq!(
        names,
        [json!("ShareX"), json!("PicGo laptop")],
        "newest first"
    );
    for t in &tokens {
        let mut keys: Vec<_> = t.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            ["created_at", "id", "last_used_at", "name", "revoked_at"]
        );
    }

    // Nor after a restart.
    let app = app.restart().await;
    let res = app.get_as("/api/v1/tokens", &session).await;
    assert_eq!(res.json()["tokens"].as_array().unwrap().len(), 2);
    assert!(!res.text().contains(&secret));
}

#[tokio::test]
async fn uploading_with_a_token_works_and_returns_the_same_result_as_a_browser_upload() {
    let app = TestApp::builder()
        .password(PASSWORD)
        .path_template("shots/{name}-{random:3}.{ext}")
        .build()
        .await;
    let session = app.login_ok(PASSWORD, false).await;
    let (_, secret) = app.create_token_ok(&session, "script").await;

    let bytes = png(10, 10, "token");
    app.random.push_base62("abc");
    let res = app
        .upload_with_token(&secret, &[("cat pic.png", &bytes)])
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    let result = &res.json()["results"][0];
    assert_eq!(result["filename"], "cat pic.png");
    assert_eq!(result["duplicate"], false);
    assert_eq!(result["restored"], false);
    assert!(result["original_uploaded_at"].is_null());
    let image = &result["image"];
    assert_eq!(
        image["path"], "shots/cat-pic-abc.png",
        "the Path Template applies"
    );
    assert_eq!(image["url"], format!("{PUBLIC_URL}/shots/cat-pic-abc.png"));
    assert_eq!(image["type"], "image/png");

    // It serves, and the Owner sees it in the Library.
    let served = app.get("/shots/cat-pic-abc.png").await;
    assert_eq!(served.status, StatusCode::OK);
    assert_eq!(&served.body[..], &bytes[..]);
    let library = app.get_as("/api/v1/images", &session).await.json();
    assert_eq!(library["total"], 1);
    assert_eq!(library["images"][0]["id"], image["id"]);

    // A Duplicate Upload with the token gets the Duplicate Upload information.
    app.clock.advance(Duration::hours(1));
    let res = app
        .upload_with_token(&secret, &[("again.png", &bytes)])
        .await;
    let result = &res.json()["results"][0];
    assert_eq!(result["duplicate"], true);
    assert_eq!(result["original_uploaded_at"], START);
    assert_eq!(result["image"]["id"], image["id"]);
    assert_eq!(result["image"]["path"], "shots/cat-pic-abc.png");

    // Several files at once, and a refused one, as in the browser.
    let jpg = jpeg(4, 4, "token");
    app.random.push_base62("xyz");
    let res = app
        .upload_with_token(&secret, &[("a.jpg", &jpg), ("notes.txt", b"hello")])
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let results = res.json()["results"].as_array().unwrap().clone();
    assert_eq!(results[0]["image"]["path"], "shots/a-xyz.jpg");
    assert_eq!(results[1]["error"]["code"], "unsupported_type");
    let res = app
        .upload_with_token(&secret, &[("notes.txt", b"hello")])
        .await;
    assert_error(&res, StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported_type");
}

#[tokio::test]
async fn a_token_upload_of_a_trashed_image_restores_it() {
    let (app, session) = app_and_session().await;
    let (_, secret) = app.create_token_ok(&session, "script").await;
    let bytes = png(3, 3, "trash");
    let image = app.upload_ok(&session, "x.png", &bytes).await;
    let res = app
        .delete_as(&format!("/api/v1/images/{}", image["id"]), &session)
        .await;
    assert_eq!(res.status, StatusCode::OK);

    let res = app.upload_with_token(&secret, &[("x.png", &bytes)]).await;
    let result = &res.json()["results"][0];
    assert_eq!(result["duplicate"], true);
    assert_eq!(result["restored"], true);
    assert!(result["image"]["trashed_at"].is_null());
    let path = image["path"].as_str().unwrap();
    assert_eq!(app.get(&format!("/{path}")).await.status, StatusCode::OK);
}

#[tokio::test]
async fn using_a_token_updates_last_used_at_most_once_a_minute() {
    let (app, session) = app_and_session().await;
    let (token, secret) = app.create_token_ok(&session, "PicGo").await;
    let (idle, _) = app.create_token_ok(&session, "idle").await;
    let id = &token["id"];

    app.clock.advance(Duration::minutes(5));
    let used = START + 5 * 60;
    let res = app
        .upload_with_token(&secret, &[("a.png", &png(2, 2, "1"))])
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(listed(&app, &session, id).await["last_used_at"], used);
    assert!(
        listed(&app, &session, &idle["id"]).await["last_used_at"].is_null(),
        "only the token used"
    );

    // Within a minute: not rewritten.
    app.clock.advance(Duration::seconds(59));
    app.upload_with_token(&secret, &[("b.png", &png(2, 2, "2"))])
        .await;
    assert_eq!(listed(&app, &session, id).await["last_used_at"], used);

    // A minute or more after the recorded use: updated.
    app.clock.advance(Duration::seconds(1));
    app.upload_with_token(&secret, &[("c.png", &png(2, 2, "3"))])
        .await;
    assert_eq!(listed(&app, &session, id).await["last_used_at"], used + 60);

    // A browser upload doesn't touch any token.
    app.clock.advance(Duration::hours(1));
    app.upload_ok(&session, "d.png", &png(2, 2, "4")).await;
    assert_eq!(listed(&app, &session, id).await["last_used_at"], used + 60);
}

#[tokio::test]
async fn a_token_is_refused_on_every_other_endpoint() {
    let (app, session) = app_and_session().await;
    let (token, secret) = app.create_token_ok(&session, "leaky").await;
    let image = app.upload_ok(&session, "a.png", &png(5, 5, "a")).await;
    let id = image["id"].clone();
    let path = image["path"].as_str().unwrap().to_owned();
    let token_id = token["id"].clone();

    let endpoints: Vec<(Method, String, Option<Value>)> = vec![
        // Library
        (Method::GET, "/api/v1/images".into(), None),
        (Method::GET, format!("/api/v1/images/{id}"), None),
        (Method::GET, format!("/api/v1/images/{id}/file"), None),
        // Paths
        (
            Method::POST,
            format!("/api/v1/images/{id}/paths"),
            Some(json!({ "path": "stolen.png" })),
        ),
        (
            Method::DELETE,
            format!("/api/v1/images/{id}/paths/{path}"),
            None,
        ),
        (
            Method::PUT,
            format!("/api/v1/images/{id}/primary"),
            Some(json!({ "path": path })),
        ),
        // Trash
        (Method::DELETE, format!("/api/v1/images/{id}"), None),
        (Method::GET, "/api/v1/trash".into(), None),
        (
            Method::POST,
            "/api/v1/trash".into(),
            Some(json!({ "ids": [id] })),
        ),
        (
            Method::POST,
            "/api/v1/trash/restore".into(),
            Some(json!({ "ids": [id] })),
        ),
        (
            Method::POST,
            "/api/v1/trash/delete".into(),
            Some(json!({ "ids": [id] })),
        ),
        (Method::DELETE, format!("/api/v1/trash/{id}"), None),
        // Settings
        (Method::GET, "/api/v1/settings".into(), None),
        (
            Method::PUT,
            "/api/v1/settings".into(),
            Some(json!({ "path_template": "{random:8}" })),
        ),
        (
            Method::POST,
            "/api/v1/settings/preview".into(),
            Some(json!({ "path_template": "{random:8}" })),
        ),
        // Tokens
        (Method::GET, "/api/v1/tokens".into(), None),
        (
            Method::POST,
            "/api/v1/tokens".into(),
            Some(json!({ "name": "another" })),
        ),
        (Method::DELETE, format!("/api/v1/tokens/{token_id}"), None),
        // Session
        (Method::GET, "/api/v1/session".into(), None),
        (Method::DELETE, "/api/v1/session".into(), None),
        (
            Method::PUT,
            "/api/v1/session/password".into(),
            Some(json!({ "current_password": "x", "new_password": "long enough" })),
        ),
        // Unknown paths and methods
        (Method::GET, "/api/v1/nope".into(), None),
        (Method::PATCH, "/api/v1/images".into(), None),
    ];
    for (method, uri, body) in endpoints {
        let res = app
            .request_with_token(method.clone(), &uri, &secret, body)
            .await;
        assert_eq!(
            res.status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri}: {}",
            res.text()
        );
        assert_eq!(res.error_code(), "unauthorized", "{method} {uri}");
    }

    // Nothing changed.
    let detail = app
        .get_as(&format!("/api/v1/images/{id}"), &session)
        .await
        .json();
    assert_eq!(detail["paths"].as_array().unwrap().len(), 1);
    assert!(detail["trashed_at"].is_null());
    let tokens = list(&app, &session).await;
    assert_eq!(tokens["tokens"].as_array().unwrap().len(), 1);
    assert!(tokens["tokens"][0]["revoked_at"].is_null());
    assert!(
        tokens["tokens"][0]["last_used_at"].is_null(),
        "refused requests are not a use"
    );
    let settings = app.get_as("/api/v1/settings", &session).await.json();
    assert_eq!(settings["path_template"], "{random:6}.{ext}");
    assert_eq!(app.get(&format!("/{path}")).await.status, StatusCode::OK);
    // The token itself still uploads.
    let res = app
        .upload_with_token(&secret, &[("b.png", &png(5, 5, "b"))])
        .await;
    assert_eq!(res.status, StatusCode::OK);
}

#[tokio::test]
async fn a_revoked_token_is_refused_and_stays_listed_as_revoked() {
    let (app, session) = app_and_session().await;
    let (token, secret) = app.create_token_ok(&session, "old laptop").await;
    let (_, kept) = app.create_token_ok(&session, "desktop").await;
    let id = &token["id"];

    app.clock.advance(Duration::hours(2));
    let res = revoke(&app, &session, id).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    let revoked_at = START + 2 * 3600;
    assert_eq!(res.json()["revoked_at"], revoked_at);
    assert_eq!(res.json()["name"], "old laptop");
    assert!(!res.text().contains(&secret));

    let res = app
        .upload_with_token(&secret, &[("a.png", &png(2, 2, "a"))])
        .await;
    assert_token_refused(&res);
    assert_eq!(
        app.get_as("/api/v1/images", &session).await.json()["total"],
        0,
        "nothing was stored"
    );

    // Listed after the active tokens, marked revoked.
    let tokens = list(&app, &session).await["tokens"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(tokens[0]["name"], "desktop");
    assert!(tokens[0]["revoked_at"].is_null());
    assert_eq!(tokens[1]["name"], "old laptop");
    assert_eq!(tokens[1]["revoked_at"], revoked_at);

    // Revoking again keeps the first date; unknown ids are 404.
    app.clock.advance(Duration::hours(1));
    let res = revoke(&app, &session, id).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.json()["revoked_at"], revoked_at);
    assert_error(
        &revoke(&app, &session, &json!(9999)).await,
        StatusCode::NOT_FOUND,
        "not_found",
    );
    assert_error(
        &app.delete_as("/api/v1/tokens/abc", &session).await,
        StatusCode::NOT_FOUND,
        "not_found",
    );

    // Still refused after a restart; the other token is unaffected.
    let app = app.restart().await;
    let res = app
        .upload_with_token(&secret, &[("a.png", &png(2, 2, "a"))])
        .await;
    assert_token_refused(&res);
    let res = app
        .upload_with_token(&kept, &[("a.png", &png(2, 2, "a"))])
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
}

#[tokio::test]
async fn invalid_tokens_are_refused() {
    let (app, session) = app_and_session().await;
    let (_, secret) = app.create_token_ok(&session, "real").await;
    let bytes = png(2, 2, "x");
    let without_prefix = secret.trim_start_matches("imgbed_").to_owned();
    // Change the last character (to one it isn't already).
    let last = if secret.ends_with('A') { 'B' } else { 'A' };
    let altered = format!("{}{last}", &secret[..secret.len() - 1]);
    for bad in [
        "imgbed_not-a-real-token",
        "nonsense",
        without_prefix.as_str(),
        altered.as_str(),
        session.token.as_str(), // a session token is not an API Token
    ] {
        let res = app.upload_with_token(bad, &[("a.png", &bytes)]).await;
        assert_token_refused(&res);
    }

    // Other schemes and malformed headers.
    for header in ["Basic dXNlcjpwYXNz", "Bearer", "Bearer ", secret.as_str()] {
        let (content_type, body) = common::multipart_body(&[("a.png", &bytes)]);
        let res = app
            .send(
                axum::http::Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/images")
                    .header("authorization", header)
                    .header("content-type", content_type)
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await;
        assert_token_refused(&res);
    }

    // The scheme is case-insensitive.
    let (content_type, body) = common::multipart_body(&[("a.png", &bytes)]);
    let res = app
        .send(
            axum::http::Request::builder()
                .method(Method::POST)
                .uri("/api/v1/images")
                .header("authorization", format!("bearer {secret}"))
                .header("content-type", content_type)
                .body(axum::body::Body::from(body))
                .unwrap(),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
}

#[tokio::test]
async fn an_authorization_header_decides_even_next_to_a_session() {
    let (app, session) = app_and_session().await;
    let (token, secret) = app.create_token_ok(&session, "x").await;
    revoke(&app, &session, &token["id"]).await;
    let (content_type, body) = common::multipart_body(&[("a.png", &png(2, 2, "a"))]);
    let res = app
        .send(
            axum::http::Request::builder()
                .method(Method::POST)
                .uri("/api/v1/images")
                .header("cookie", session.cookie_header())
                .header("authorization", format!("Bearer {secret}"))
                .header("content-type", content_type)
                .body(axum::body::Body::from(body))
                .unwrap(),
        )
        .await;
    assert_token_refused(&res);
}

#[tokio::test]
async fn tokens_survive_a_password_change() {
    let (app, session) = app_and_session().await;
    let (_, secret) = app.create_token_ok(&session, "PicGo").await;
    app.set_password("a brand new password").await;
    assert!(!app.is_valid(&session).await, "sessions end");
    let res = app
        .upload_with_token(&secret, &[("a.png", &png(2, 2, "a"))])
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
}

#[tokio::test]
async fn names_are_required_and_unique_among_active_tokens() {
    let (app, session) = app_and_session().await;
    for body in [
        json!({ "name": "" }),
        json!({ "name": "   " }),
        json!({ "name": "a\tb" }),
        json!({ "name": "x".repeat(101) }),
        json!({}),
        json!({ "name": 5 }),
        json!({ "name": "ok", "scope": "all" }),
    ] {
        let res = create(&app, Some(&session), body).await;
        assert_error(&res, StatusCode::BAD_REQUEST, "bad_request");
    }
    let res = create(&app, Some(&session), json!({ "name": "x".repeat(100) })).await;
    assert_eq!(res.status, StatusCode::CREATED);
    let res = create(&app, Some(&session), json!({ "name": "ünïcode 📷" })).await;
    assert_eq!(res.status, StatusCode::CREATED);
    assert_eq!(res.json()["name"], "ünïcode 📷");

    let (first, _) = app.create_token_ok(&session, "PicGo").await;
    let res = create(&app, Some(&session), json!({ "name": " PicGo " })).await;
    assert_error(&res, StatusCode::CONFLICT, "token_name_taken");
    // Names that differ in case are different names.
    app.create_token_ok(&session, "picgo").await;

    // Once revoked, the name is free again.
    revoke(&app, &session, &first["id"]).await;
    let (second, _) = app.create_token_ok(&session, "PicGo").await;
    assert_ne!(second["id"], first["id"]);
    let names: Vec<_> = list(&app, &session).await["tokens"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["name"] == "PicGo")
        .map(|t| t["revoked_at"].is_null())
        .collect();
    assert_eq!(
        names,
        [true, false],
        "the new one active, the old one revoked"
    );
}

#[tokio::test]
async fn token_management_requires_a_session() {
    let (app, session) = app_and_session().await;
    let (token, _) = app.create_token_ok(&session, "PicGo").await;
    let forged = Session {
        token: "not-a-real-session".into(),
    };
    let id = &token["id"];
    for s in [None, Some(&forged)] {
        let res = app.request(Method::GET, "/api/v1/tokens", s, None).await;
        assert_error(&res, StatusCode::UNAUTHORIZED, "unauthorized");
        let res = create(&app, s, json!({ "name": "sneaky" })).await;
        assert_error(&res, StatusCode::UNAUTHORIZED, "unauthorized");
        let res = app
            .request(Method::DELETE, &format!("/api/v1/tokens/{id}"), s, None)
            .await;
        assert_error(&res, StatusCode::UNAUTHORIZED, "unauthorized");
    }
    // After logout, too.
    app.delete_as("/api/v1/session", &session).await;
    let res = app.get_as("/api/v1/tokens", &session).await;
    assert_error(&res, StatusCode::UNAUTHORIZED, "unauthorized");

    let other = app.login_ok(PASSWORD, false).await;
    let tokens = list(&app, &other).await;
    let tokens = tokens["tokens"].as_array().unwrap();
    assert_eq!(tokens.len(), 1, "nothing was created");
    assert!(tokens[0]["revoked_at"].is_null(), "nothing was revoked");
}
