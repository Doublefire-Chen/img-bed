//! Path Template settings: reading, saving and previewing the template, and
//! its effect on uploads.

mod common;

use axum::http::{Method, StatusCode};
use common::fixtures::{jpeg, png};
use common::{Session, TestApp, TestResponse};
use serde_json::{Value, json};
use time::Duration;

const PASSWORD: &str = "correct horse battery";

async fn app_and_session() -> (TestApp, Session) {
    let app = TestApp::with_password(PASSWORD).await;
    let session = app.login_ok(PASSWORD, false).await;
    (app, session)
}

async fn put(app: &TestApp, session: &Session, body: Value) -> TestResponse {
    app.request(Method::PUT, "/api/v1/settings", Some(session), Some(body))
        .await
}

async fn save(app: &TestApp, session: &Session, template: &str) -> TestResponse {
    put(app, session, json!({ "path_template": template })).await
}

async fn save_ok(app: &TestApp, session: &Session, template: &str) -> Value {
    let res = save(app, session, template).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    res.json()
}

async fn settings(app: &TestApp, session: &Session) -> Value {
    let res = app.get_as("/api/v1/settings", session).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    res.json()
}

async fn preview(app: &TestApp, session: &Session, template: &str) -> TestResponse {
    app.request(
        Method::POST,
        "/api/v1/settings/preview",
        Some(session),
        Some(json!({ "path_template": template })),
    )
    .await
}

fn assert_error(res: &TestResponse, status: StatusCode, code: &str) {
    assert_eq!(res.status, status, "{}", res.text());
    assert_eq!(res.json()["error"]["code"], code, "{}", res.text());
    assert!(
        !res.json()["error"]["message"].as_str().unwrap().is_empty(),
        "an explanation is given"
    );
}

#[tokio::test]
async fn the_default_is_short_with_extension_and_the_presets_are_listed() {
    let (app, session) = app_and_session().await;
    let s = settings(&app, &session).await;
    assert_eq!(s["path_template"], "{random:6}.{ext}");
    assert_eq!(s["preset"], "short_ext");

    let presets = s["presets"].as_array().unwrap();
    let listed: Vec<_> = presets
        .iter()
        .map(|p| {
            (
                p["id"].as_str().unwrap(),
                p["name"].as_str().unwrap(),
                p["template"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        listed,
        [
            ("short", "Short", "{random:6}"),
            ("short_ext", "Short with extension", "{random:6}.{ext}"),
            ("dated", "Dated", "{Y}/{m}/{d}/{random:13}.{ext}"),
            ("filename", "Filename", "{name}-{random:4}.{ext}"),
        ]
    );
    // Examples are rendered for an upload of "holiday photo.jpg" now.
    let example = |i: usize| presets[i]["example"].as_str().unwrap().to_owned();
    assert_eq!(example(0).len(), 6);
    assert!(example(1).ends_with(".jpg") && example(1).len() == 10);
    assert!(example(2).starts_with("2026/01/01/") && example(2).ends_with(".jpg"));
    assert!(example(3).starts_with("holiday-photo-") && example(3).ends_with(".jpg"));
    assert_eq!(s["example"], example(1).as_str());
}

#[tokio::test]
async fn each_preset_can_be_saved_and_new_uploads_follow_it() {
    let (app, session) = app_and_session().await;
    // (template, forced random characters, expected Path of "My Cat.png")
    let cases = [
        ("short", "{random:6}", "abcdef", "abcdef"),
        ("short_ext", "{random:6}.{ext}", "ghijkl", "ghijkl.png"),
        (
            "dated",
            "{Y}/{m}/{d}/{random:13}.{ext}",
            "ABCDEFGHIJKLM",
            "2026/01/01/ABCDEFGHIJKLM.png",
        ),
        (
            "filename",
            "{name}-{random:4}.{ext}",
            "wxyz",
            "My-Cat-wxyz.png",
        ),
    ];
    for (i, (id, template, random, expected)) in cases.into_iter().enumerate() {
        let saved = save_ok(&app, &session, template).await;
        assert_eq!(saved["path_template"], template);
        assert_eq!(saved["preset"], id);
        let s = settings(&app, &session).await;
        assert_eq!(s["path_template"], template);
        assert_eq!(s["preset"], id);

        app.random.push_base62(random);
        let bytes = png(2, 2, &format!("preset {i}"));
        let image = app.upload_ok(&session, "My Cat.png", &bytes).await;
        assert_eq!(image["path"], expected, "{id}");
        assert_eq!(&app.get(&format!("/{expected}")).await.body[..], &bytes[..]);
    }
}

#[tokio::test]
async fn a_custom_template_can_be_saved_and_is_used_for_new_uploads() {
    let (app, session) = app_and_session().await;
    let saved = save_ok(&app, &session, "pics/{Y}-{m}/{name}_{hash:8}.{ext}").await;
    assert_eq!(saved["preset"], Value::Null);
    assert!(
        saved["example"]
            .as_str()
            .unwrap()
            .starts_with("pics/2026-01/holiday-photo_")
    );

    let bytes = jpeg(3, 3, "custom");
    let image = app.upload_ok(&session, "beach day.jpeg", &bytes).await;
    let sha = image["sha256"].as_str().unwrap();
    assert_eq!(
        image["path"],
        format!("pics/2026-01/beach-day_{}.jpg", &sha[..8])
    );
    let s = settings(&app, &session).await;
    assert_eq!(s["path_template"], "pics/{Y}-{m}/{name}_{hash:8}.{ext}");
    assert_eq!(s["preset"], Value::Null);
}

#[tokio::test]
async fn a_template_without_random_or_hash_is_refused() {
    let (app, session) = app_and_session().await;
    for template in ["{name}.{ext}", "{Y}/{m}/{d}/{name}", "static.png"] {
        let res = save(&app, &session, template).await;
        assert_error(&res, StatusCode::BAD_REQUEST, "template_not_unique");
        let message = res.json()["error"]["message"].as_str().unwrap().to_owned();
        assert!(
            message.contains("{random:N}") && message.contains("{hash:N}"),
            "{message}"
        );
    }
    assert_eq!(
        settings(&app, &session).await["path_template"],
        "{random:6}.{ext}"
    );
}

#[tokio::test]
async fn an_unknown_variable_is_refused() {
    let (app, session) = app_and_session().await;
    for template in ["{foo}-{random:6}", "{random:6}.{EXT}", "{ext:3}{random:6}"] {
        let res = save(&app, &session, template).await;
        assert_error(&res, StatusCode::BAD_REQUEST, "unknown_variable");
    }
    let res = save(&app, &session, "{foo}-{random:6}").await;
    assert!(
        res.json()["error"]["message"]
            .as_str()
            .unwrap()
            .contains("{foo}")
    );
    assert_eq!(
        settings(&app, &session).await["path_template"],
        "{random:6}.{ext}"
    );
}

#[tokio::test]
async fn other_invalid_templates_are_refused() {
    let (app, session) = app_and_session().await;
    for template in [
        "",
        "{random}",
        "{random:0}",
        "{random:65}",
        "{random:6",
        "a b{random:6}",
        "/{random:6}",
        "a//{random:6}",
        "../{random:6}",
        "api/{random:6}",
        "admin/{random:6}.{ext}",
    ] {
        let res = save(&app, &session, template).await;
        assert_error(&res, StatusCode::BAD_REQUEST, "invalid_template");
    }
    assert_eq!(
        settings(&app, &session).await["path_template"],
        "{random:6}.{ext}"
    );
}

#[tokio::test]
async fn changing_the_template_does_not_change_existing_paths() {
    let (app, session) = app_and_session().await;
    app.random.push_base62("oldone");
    let old_bytes = png(5, 5, "old");
    let old = app.upload_ok(&session, "old.png", &old_bytes).await;
    assert_eq!(old["path"], "oldone.png");
    let id = old["id"].as_i64().unwrap();

    save_ok(&app, &session, "new/{random:8}").await;
    app.clock.advance(Duration::minutes(1));

    // The existing Image keeps exactly its Paths, and they still serve it.
    let detail = app
        .get_as(&format!("/api/v1/images/{id}"), &session)
        .await
        .json();
    assert_eq!(detail["path"], "oldone.png");
    assert_eq!(detail["paths"], old["paths"]);
    assert_eq!(&app.get("/oldone.png").await.body[..], &old_bytes[..]);

    // A Duplicate Upload of it is not given a Path from the new template.
    let dup = app.upload_ok(&session, "old again.png", &old_bytes).await;
    assert_eq!(dup["paths"], old["paths"]);

    // The very next new upload uses the new template.
    app.random.push_base62("NEWPATH1");
    let new = app.upload_ok(&session, "new.png", &png(6, 6, "new")).await;
    assert_eq!(new["path"], "new/NEWPATH1");
}

#[tokio::test]
async fn the_template_survives_a_restart() {
    let (app, session) = app_and_session().await;
    save_ok(&app, &session, "{hash:10}").await;
    let app = app.restart().await;
    let session = app.login_ok(PASSWORD, false).await;
    assert_eq!(settings(&app, &session).await["path_template"], "{hash:10}");
    let image = app
        .upload_ok(&session, "a.png", &png(1, 1, "restart"))
        .await;
    let sha = image["sha256"].as_str().unwrap();
    assert_eq!(image["path"], &sha[..10]);
}

#[tokio::test]
async fn the_test_builder_stores_the_template_as_a_setting() {
    let app = TestApp::builder()
        .password(PASSWORD)
        .path_template("{random:6}")
        .build()
        .await;
    let session = app.login_ok(PASSWORD, false).await;
    let s = settings(&app, &session).await;
    assert_eq!(s["path_template"], "{random:6}");
    assert_eq!(s["preset"], "short");
}

#[tokio::test]
async fn preview_shows_an_example_without_saving() {
    let (app, session) = app_and_session().await;
    let res = preview(&app, &session, "{Y}/{name}-{random:5}.{ext}").await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    let body = res.json();
    assert_eq!(body["path_template"], "{Y}/{name}-{random:5}.{ext}");
    assert_eq!(body["preset"], Value::Null);
    let example = body["example"].as_str().unwrap();
    assert!(example.starts_with("2026/holiday-photo-"), "{example}");
    assert!(
        example.ends_with(".jpg") && example.len() == 28,
        "{example}"
    );

    // The same template previews the same example while the Owner types.
    let again = preview(&app, &session, "{Y}/{name}-{random:5}.{ext}")
        .await
        .json();
    assert_eq!(again["example"], example);

    let res = preview(&app, &session, "{random:13}").await;
    assert_eq!(res.json()["preset"], Value::Null);
    let res = preview(&app, &session, "{random:6}").await;
    assert_eq!(res.json()["preset"], "short");

    // Nothing was saved.
    assert_eq!(
        settings(&app, &session).await["path_template"],
        "{random:6}.{ext}"
    );
}

#[tokio::test]
async fn preview_reports_the_same_errors_as_saving() {
    let (app, session) = app_and_session().await;
    for (template, code) in [
        ("{name}.{ext}", "template_not_unique"),
        ("{nope}{random:6}", "unknown_variable"),
        ("{random:6", "invalid_template"),
    ] {
        let res = preview(&app, &session, template).await;
        assert_error(&res, StatusCode::BAD_REQUEST, code);
    }
}

#[tokio::test]
async fn bad_bodies_are_bad_requests() {
    let (app, session) = app_and_session().await;
    let res = put(&app, &session, json!({ "path_template": 5 })).await;
    assert_error(&res, StatusCode::BAD_REQUEST, "bad_request");
    let res = put(&app, &session, json!({ "path_tempalte": "{random:6}" })).await;
    assert_error(&res, StatusCode::BAD_REQUEST, "bad_request");
    // An empty update changes nothing.
    let res = put(&app, &session, json!({})).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.json()["path_template"], "{random:6}.{ext}");
}

#[tokio::test]
async fn the_settings_endpoints_require_a_session() {
    let app = TestApp::with_password(PASSWORD).await;
    let body = json!({ "path_template": "{random:6}" });
    for (method, uri, body) in [
        (Method::GET, "/api/v1/settings", None),
        (Method::PUT, "/api/v1/settings", Some(body.clone())),
        (Method::POST, "/api/v1/settings/preview", Some(body.clone())),
    ] {
        let res = app.request(method, uri, None, body).await;
        assert_error(&res, StatusCode::UNAUTHORIZED, "unauthorized");
    }
    // Nothing changed.
    let session = app.login_ok(PASSWORD, false).await;
    assert_eq!(
        settings(&app, &session).await["path_template"],
        "{random:6}.{ext}"
    );
}
