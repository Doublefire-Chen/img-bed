//! Path management: adding, removing and setting the Primary Path.

mod common;

use axum::http::{Method, StatusCode};
use common::fixtures::{gif, png};
use common::{PUBLIC_URL, Session, TestApp, TestResponse};
use serde_json::{Value, json};

const PASSWORD: &str = "correct horse battery";

async fn app_and_session() -> (TestApp, Session) {
    let app = TestApp::with_password(PASSWORD).await;
    let session = app.login_ok(PASSWORD, false).await;
    (app, session)
}

/// Uploads a PNG whose first Path is `<random>.png`; returns the Image.
async fn upload(app: &TestApp, session: &Session, random: &str, salt: &str) -> Value {
    app.random.push_base62(random);
    let image = app
        .upload_ok(session, &format!("{salt}.png"), &png(4, 4, salt))
        .await;
    assert_eq!(image["path"], format!("{random}.png"));
    image
}

fn id(image: &Value) -> i64 {
    image["id"].as_i64().unwrap()
}

/// Percent-encodes a Path for use in a URL, keeping `/`.
fn encode(path: &str) -> String {
    path.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

async fn add(app: &TestApp, session: &Session, image: i64, path: &str) -> TestResponse {
    app.request(
        Method::POST,
        &format!("/api/v1/images/{image}/paths"),
        Some(session),
        Some(json!({ "path": path })),
    )
    .await
}

async fn add_ok(app: &TestApp, session: &Session, image: i64, path: &str) -> Value {
    let res = add(app, session, image, path).await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.text());
    res.json()
}

async fn remove(app: &TestApp, session: &Session, image: i64, path: &str) -> TestResponse {
    app.delete_as(
        &format!("/api/v1/images/{image}/paths/{}", encode(path)),
        session,
    )
    .await
}

async fn set_primary(app: &TestApp, session: &Session, image: i64, path: &str) -> TestResponse {
    app.request(
        Method::PUT,
        &format!("/api/v1/images/{image}/primary"),
        Some(session),
        Some(json!({ "path": path })),
    )
    .await
}

async fn detail(app: &TestApp, session: &Session, image: i64) -> Value {
    let res = app
        .get_as(&format!("/api/v1/images/{image}"), session)
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    res.json()
}

/// The Image's Paths in order, with `*` after the Primary one.
fn paths(image: &Value) -> Vec<String> {
    image["paths"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let path = p["path"].as_str().unwrap().to_owned();
            if p["is_primary"] == true {
                path + "*"
            } else {
                path
            }
        })
        .collect()
}

/// Serves `/<path>` (encoded) and returns the status and body.
async fn serve(app: &TestApp, path: &str) -> TestResponse {
    app.get(&format!("/{}", encode(path))).await
}

#[tokio::test]
async fn an_added_path_serves_the_image_and_the_old_path_keeps_working() {
    let (app, session) = app_and_session().await;
    let image = upload(&app, &session, "Abc123", "a").await;
    let bytes = png(4, 4, "a");

    let updated = add_ok(&app, &session, id(&image), "cat.png").await;
    assert_eq!(paths(&updated), ["Abc123.png*", "cat.png"]);
    assert_eq!(updated["paths"][1]["url"], format!("{PUBLIC_URL}/cat.png"));
    assert_eq!(
        updated["path"], "Abc123.png",
        "the Primary Path is unchanged"
    );
    assert_eq!(
        paths(&detail(&app, &session, id(&image)).await),
        paths(&updated)
    );

    for path in ["cat.png", "Abc123.png"] {
        let res = serve(&app, path).await;
        assert_eq!(res.status, StatusCode::OK, "/{path}");
        assert_eq!(res.body.as_ref(), bytes.as_slice());
        assert_eq!(res.header("content-type"), Some("image/png"));
    }
}

#[tokio::test]
async fn paths_with_folders_and_other_scripts_can_be_added() {
    let (app, session) = app_and_session().await;
    let image = upload(&app, &session, "Abc123", "a").await;

    add_ok(&app, &session, id(&image), "2019/05/old-link.png").await;
    let updated = add_ok(&app, &session, id(&image), "截图-猫.png").await;
    assert_eq!(
        updated["paths"][2]["url"],
        format!("{PUBLIC_URL}/%E6%88%AA%E5%9B%BE-%E7%8C%AB.png")
    );
    // Percent-encoded input is stored decoded.
    let updated = add_ok(&app, &session, id(&image), "%C3%BCber.png").await;
    assert_eq!(updated["paths"][3]["path"], "über.png");

    for path in ["2019/05/old-link.png", "截图-猫.png", "über.png"] {
        assert_eq!(serve(&app, path).await.status, StatusCode::OK, "/{path}");
    }
}

#[tokio::test]
async fn invalid_paths_are_refused() {
    let (app, session) = app_and_session().await;
    let image = upload(&app, &session, "Abc123", "a").await;
    let long = "a".repeat(256);
    for path in [
        "",
        "/cat.png",
        "cat/",
        "a//b.png",
        "../cat.png",
        "a/./b.png",
        "a b.png",
        "what?.png",
        "%ff.png",
        long.as_str(),
    ] {
        let res = add(&app, &session, id(&image), path).await;
        assert_eq!(
            res.status,
            StatusCode::BAD_REQUEST,
            "{path:?}: {}",
            res.text()
        );
        assert_eq!(res.error_code(), "invalid_path", "{path:?}");
    }
    assert_eq!(
        paths(&detail(&app, &session, id(&image)).await),
        ["Abc123.png*"]
    );
}

#[tokio::test]
async fn paths_under_a_reserved_prefix_are_refused() {
    let (app, session) = app_and_session().await;
    let image = upload(&app, &session, "Abc123", "a").await;
    for path in [
        "admin/cat.png",
        "api/cat.png",
        "admin",
        "api",
        "api/v1/info",
        "%61dmin/x",
    ] {
        let res = add(&app, &session, id(&image), path).await;
        assert_eq!(
            res.status,
            StatusCode::BAD_REQUEST,
            "{path}: {}",
            res.text()
        );
        assert_eq!(res.error_code(), "reserved_path", "{path}");
        assert!(
            res.json()["error"]["message"]
                .as_str()
                .unwrap()
                .contains("Reserved Prefix")
        );
    }
    // Lookalikes are ordinary Paths.
    add_ok(&app, &session, id(&image), "administrator.png").await;
    add_ok(&app, &session, id(&image), "apis/x.png").await;
    assert_eq!(paths(&detail(&app, &session, id(&image)).await).len(), 3);
}

#[tokio::test]
async fn a_path_already_in_use_is_refused_as_taken() {
    let (app, session) = app_and_session().await;
    let first = upload(&app, &session, "First1", "a").await;
    let second = upload(&app, &session, "Secnd2", "b").await;
    add_ok(&app, &session, id(&first), "cat.png").await;

    // Taken by another Image, as an added Path or a generated one.
    for path in ["cat.png", "First1.png"] {
        let res = add(&app, &session, id(&second), path).await;
        assert_eq!(res.status, StatusCode::CONFLICT, "{}", res.text());
        assert_eq!(res.error_code(), "path_taken");
        assert_eq!(
            res.json()["error"]["message"],
            format!("/{path} is already taken by another Image")
        );
    }
    // Already one of this Image's own Paths.
    let res = add(&app, &session, id(&first), "cat.png").await;
    assert_eq!(res.status, StatusCode::CONFLICT);
    assert_eq!(res.error_code(), "path_taken");
    assert_eq!(
        res.json()["error"]["message"],
        "/cat.png is already a Path of this Image"
    );

    // Paths are case-sensitive, so a different case is a different Path.
    add_ok(&app, &session, id(&second), "Cat.png").await;
    assert_eq!(
        paths(&detail(&app, &session, id(&first)).await),
        ["First1.png*", "cat.png"]
    );
    assert_eq!(
        serve(&app, "cat.png").await.body.as_ref(),
        png(4, 4, "a").as_slice()
    );
    assert_eq!(
        serve(&app, "Cat.png").await.body.as_ref(),
        png(4, 4, "b").as_slice()
    );
}

#[tokio::test]
async fn a_removed_path_is_404_and_free_for_another_image() {
    let (app, session) = app_and_session().await;
    let first = upload(&app, &session, "First1", "a").await;
    let second = upload(&app, &session, "Secnd2", "b").await;
    add_ok(&app, &session, id(&first), "avatar.png").await;
    assert_eq!(serve(&app, "avatar.png").await.status, StatusCode::OK);

    let res = remove(&app, &session, id(&first), "avatar.png").await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    assert_eq!(paths(&res.json()), ["First1.png*"]);
    assert_eq!(
        serve(&app, "avatar.png").await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(serve(&app, "First1.png").await.status, StatusCode::OK);

    let moved = add_ok(&app, &session, id(&second), "avatar.png").await;
    assert_eq!(paths(&moved), ["Secnd2.png*", "avatar.png"]);
    let res = serve(&app, "avatar.png").await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body.as_ref(), png(4, 4, "b").as_slice());
}

#[tokio::test]
async fn a_generated_path_can_be_removed_and_reused() {
    let (app, session) = app_and_session().await;
    let first = upload(&app, &session, "First1", "a").await;
    let second = upload(&app, &session, "Secnd2", "b").await;
    add_ok(&app, &session, id(&first), "cat.png").await;
    remove(&app, &session, id(&first), "First1.png").await;
    let updated = add_ok(&app, &session, id(&second), "First1.png").await;
    assert_eq!(paths(&updated), ["Secnd2.png*", "First1.png"]);
    assert_eq!(
        serve(&app, "First1.png").await.body.as_ref(),
        png(4, 4, "b").as_slice()
    );
}

#[tokio::test]
async fn removing_the_last_path_is_refused() {
    let (app, session) = app_and_session().await;
    let image = upload(&app, &session, "Abc123", "a").await;
    let res = remove(&app, &session, id(&image), "Abc123.png").await;
    assert_eq!(res.status, StatusCode::CONFLICT, "{}", res.text());
    assert_eq!(res.error_code(), "last_path");
    assert_eq!(
        paths(&detail(&app, &session, id(&image)).await),
        ["Abc123.png*"]
    );
    assert_eq!(serve(&app, "Abc123.png").await.status, StatusCode::OK);

    // With a second Path, either can go, but not both.
    add_ok(&app, &session, id(&image), "cat.png").await;
    assert_eq!(
        remove(&app, &session, id(&image), "cat.png").await.status,
        StatusCode::OK
    );
    let res = remove(&app, &session, id(&image), "Abc123.png").await;
    assert_eq!(res.error_code(), "last_path");
}

#[tokio::test]
async fn removing_the_primary_path_promotes_the_oldest_remaining_one() {
    let (app, session) = app_and_session().await;
    let image = upload(&app, &session, "Abc123", "a").await;
    add_ok(&app, &session, id(&image), "one.png").await;
    add_ok(&app, &session, id(&image), "two.png").await;

    let res = remove(&app, &session, id(&image), "Abc123.png").await;
    assert_eq!(res.status, StatusCode::OK);
    let updated = res.json();
    assert_eq!(paths(&updated), ["one.png*", "two.png"]);
    assert_eq!(updated["path"], "one.png");
    assert_eq!(updated["url"], format!("{PUBLIC_URL}/one.png"));
    assert_eq!(
        serve(&app, "Abc123.png").await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn removing_a_path_that_is_not_the_images_is_404() {
    let (app, session) = app_and_session().await;
    let first = upload(&app, &session, "First1", "a").await;
    let second = upload(&app, &session, "Secnd2", "b").await;
    add_ok(&app, &session, id(&second), "cat.png").await;

    for (image, path) in [
        (id(&first), "cat.png"),
        (id(&first), "nope.png"),
        (999, "cat.png"),
    ] {
        let res = remove(&app, &session, image, path).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "{image} {path}");
    }
    // Nothing was removed from the other Image.
    assert_eq!(serve(&app, "cat.png").await.status, StatusCode::OK);
}

#[tokio::test]
async fn paths_with_folders_and_other_scripts_can_be_removed() {
    let (app, session) = app_and_session().await;
    let image = upload(&app, &session, "Abc123", "a").await;
    add_ok(&app, &session, id(&image), "2019/05/old.png").await;
    add_ok(&app, &session, id(&image), "截图.png").await;
    for path in ["2019/05/old.png", "截图.png"] {
        let res = remove(&app, &session, id(&image), path).await;
        assert_eq!(res.status, StatusCode::OK, "{path}: {}", res.text());
        assert_eq!(serve(&app, path).await.status, StatusCode::NOT_FOUND);
    }
    assert_eq!(
        paths(&detail(&app, &session, id(&image)).await),
        ["Abc123.png*"]
    );
}

#[tokio::test]
async fn setting_primary_moves_the_marker() {
    let (app, session) = app_and_session().await;
    let image = upload(&app, &session, "Abc123", "a").await;
    add_ok(&app, &session, id(&image), "one.png").await;
    add_ok(&app, &session, id(&image), "two.png").await;

    let res = set_primary(&app, &session, id(&image), "two.png").await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    let updated = res.json();
    assert_eq!(paths(&updated), ["two.png*", "Abc123.png", "one.png"]);
    assert_eq!(updated["path"], "two.png");
    assert_eq!(updated["url"], format!("{PUBLIC_URL}/two.png"));
    assert_eq!(
        paths(&detail(&app, &session, id(&image)).await),
        paths(&updated)
    );

    // Again, and setting the current Primary changes nothing.
    set_primary(&app, &session, id(&image), "one.png").await;
    let res = set_primary(&app, &session, id(&image), "one.png").await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(paths(&res.json()), ["one.png*", "Abc123.png", "two.png"]);

    // The Library shows the new Primary too.
    let library = app.get_as("/api/v1/images", &session).await.json();
    assert_eq!(library["images"][0]["path"], "one.png");

    // Every Path still serves the Image.
    for path in ["Abc123.png", "one.png", "two.png"] {
        assert_eq!(serve(&app, path).await.status, StatusCode::OK, "/{path}");
    }
}

#[tokio::test]
async fn setting_primary_to_another_images_path_is_404() {
    let (app, session) = app_and_session().await;
    let first = upload(&app, &session, "First1", "a").await;
    let second = upload(&app, &session, "Secnd2", "b").await;

    for (image, path) in [
        (id(&first), "Secnd2.png"),
        (id(&first), "nope.png"),
        (999, "x"),
    ] {
        let res = set_primary(&app, &session, image, path).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "{image} {path}");
    }
    // Both Images keep exactly their own Primary.
    assert_eq!(
        paths(&detail(&app, &session, id(&first)).await),
        ["First1.png*"]
    );
    assert_eq!(
        paths(&detail(&app, &session, id(&second)).await),
        ["Secnd2.png*"]
    );
}

#[tokio::test]
async fn adding_to_an_unknown_image_is_404() {
    let (app, session) = app_and_session().await;
    for image in ["999", "abc"] {
        let res = app
            .request(
                Method::POST,
                &format!("/api/v1/images/{image}/paths"),
                Some(&session),
                Some(json!({ "path": "cat.png" })),
            )
            .await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "{image}");
    }
    assert_eq!(serve(&app, "cat.png").await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_malformed_body_is_a_bad_request() {
    let (app, session) = app_and_session().await;
    let image = upload(&app, &session, "Abc123", "a").await;
    for uri in [
        format!("/api/v1/images/{}/paths", id(&image)),
        format!("/api/v1/images/{}/primary", id(&image)),
    ] {
        let method = if uri.ends_with("paths") {
            Method::POST
        } else {
            Method::PUT
        };
        let res = app
            .request(method, &uri, Some(&session), Some(json!({ "nope": 1 })))
            .await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "{uri}");
        assert_eq!(res.error_code(), "bad_request");
    }
}

#[tokio::test]
async fn search_finds_an_image_by_a_non_primary_path() {
    let (app, session) = app_and_session().await;
    let cat = upload(&app, &session, "Abc123", "a").await;
    let _other = upload(&app, &session, "Xyz789", "b").await;
    add_ok(&app, &session, id(&cat), "fluffy-kitten.png").await;

    let page = app.get_as("/api/v1/images?q=kitten", &session).await.json();
    let found: Vec<_> = page["images"].as_array().unwrap().iter().map(id).collect();
    assert_eq!(found, [id(&cat)]);
    // The Primary Path is still the generated one.
    assert_eq!(page["images"][0]["path"], "Abc123.png");
}

#[tokio::test]
async fn path_management_needs_an_owner_session() {
    let (app, session) = app_and_session().await;
    let image = upload(&app, &session, "Abc123", "a").await;
    let base = format!("/api/v1/images/{}", id(&image));
    let body = Some(json!({ "path": "cat.png" }));
    for (method, uri) in [
        (Method::POST, format!("{base}/paths")),
        (Method::DELETE, format!("{base}/paths/Abc123.png")),
        (Method::PUT, format!("{base}/primary")),
    ] {
        let res = app.request(method.clone(), &uri, None, body.clone()).await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED, "{method} {uri}");
        assert_eq!(res.error_code(), "unauthorized");
    }
    assert_eq!(serve(&app, "cat.png").await.status, StatusCode::NOT_FOUND);
    assert_eq!(
        paths(&detail(&app, &session, id(&image)).await),
        ["Abc123.png*"]
    );
}

#[tokio::test]
async fn gif_and_png_images_keep_their_own_paths() {
    // Two Images, each with several Paths: operations on one never touch
    // the other, and each keeps exactly one Primary.
    let (app, session) = app_and_session().await;
    app.random.push_base62("GifGif");
    let g = app.upload_ok(&session, "g.gif", &gif(2, 2, "g")).await;
    let p = upload(&app, &session, "PngPng", "p").await;
    add_ok(&app, &session, id(&g), "g2.gif").await;
    add_ok(&app, &session, id(&p), "p2.png").await;
    set_primary(&app, &session, id(&g), "g2.gif").await;
    remove(&app, &session, id(&p), "PngPng.png").await;
    assert_eq!(
        paths(&detail(&app, &session, id(&g)).await),
        ["g2.gif*", "GifGif.gif"]
    );
    assert_eq!(paths(&detail(&app, &session, id(&p)).await), ["p2.png*"]);
}
