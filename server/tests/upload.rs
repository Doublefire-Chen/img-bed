//! Uploading Images: results, type detection, rejection, first Paths.

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::fixtures::{self, png};
use common::{PUBLIC_URL, TestApp};
use serde_json::Value;
use time::Duration;

const PASSWORD: &str = "correct horse battery";

async fn app_and_session() -> (TestApp, common::Session) {
    let app = TestApp::with_password(PASSWORD).await;
    let session = app.login_ok(PASSWORD, false).await;
    (app, session)
}

fn is_default_path(path: &str, ext: &str) -> bool {
    let Some(random) = path.strip_suffix(&format!(".{ext}")) else {
        return false;
    };
    random.len() == 6 && random.bytes().all(|b| b.is_ascii_alphanumeric())
}

#[tokio::test]
async fn upload_returns_the_image_with_its_path_and_full_url() {
    let (app, session) = app_and_session().await;
    let bytes = png(640, 480, "a");
    let res = app
        .upload_as(&session, &[("Holiday Photo.png", &bytes)])
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());

    let body = res.json();
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    let result = &results[0];
    assert_eq!(result["filename"], "Holiday Photo.png");
    assert_eq!(result["duplicate"], false);
    assert!(result["original_uploaded_at"].is_null());

    let image = &result["image"];
    let path = image["path"].as_str().unwrap();
    assert!(is_default_path(path, "png"), "{path}");
    assert_eq!(image["url"], format!("{PUBLIC_URL}/{path}"));
    assert_eq!(image["type"], "image/png");
    assert_eq!(image["size"], bytes.len());
    assert_eq!(image["width"], 640);
    assert_eq!(image["height"], 480);
    assert_eq!(image["original_filename"], "Holiday Photo.png");
    assert_eq!(image["uploaded_at"], common::START);
    assert_eq!(image["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(
        image["paths"],
        serde_json::json!([{ "path": path, "url": image["url"], "is_primary": true }])
    );
}

#[tokio::test]
async fn several_files_in_one_upload_get_one_result_each_in_order() {
    let (app, session) = app_and_session().await;
    let (a, b, c) = (png(1, 1, "a"), fixtures::gif(2, 2, "b"), fixtures::svg("c"));
    let res = app
        .upload_as(&session, &[("a.png", &a), ("b.gif", &b), ("c.svg", &c)])
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    let body = res.json();
    let results = body["results"].as_array().unwrap();
    let names: Vec<_> = results.iter().map(|r| r["filename"].clone()).collect();
    assert_eq!(names, ["a.png", "b.gif", "c.svg"]);
    let paths: Vec<_> = results
        .iter()
        .map(|r| r["image"]["path"].as_str().unwrap().to_owned())
        .collect();
    assert!(is_default_path(&paths[0], "png"));
    assert!(is_default_path(&paths[1], "gif"));
    assert!(is_default_path(&paths[2], "svg"));
    for (path, bytes) in paths.iter().zip([&a, &b, &c]) {
        assert_eq!(&app.get(&format!("/{path}")).await.body[..], &bytes[..]);
    }
}

#[tokio::test]
async fn type_is_judged_by_content_not_by_name() {
    let (app, session) = app_and_session().await;
    let image = app
        .upload_ok(&session, "photo.jpg", &png(10, 20, "png in disguise"))
        .await;
    assert_eq!(image["type"], "image/png");
    assert!(is_default_path(image["path"].as_str().unwrap(), "png"));
    let served = app
        .get(&format!("/{}", image["path"].as_str().unwrap()))
        .await;
    assert_eq!(served.header("content-type"), Some("image/png"));

    let image = app
        .upload_ok(&session, "noext", &fixtures::jpeg(3, 4, "jpeg"))
        .await;
    assert_eq!(image["type"], "image/jpeg");
    assert!(is_default_path(image["path"].as_str().unwrap(), "jpg"));
}

#[tokio::test]
async fn every_accepted_type_is_stored_with_its_extension_and_size() {
    let (app, session) = app_and_session().await;
    let cases: Vec<(Vec<u8>, &str, &str, Value, Value)> = vec![
        (
            fixtures::jpeg(800, 600, ""),
            "image/jpeg",
            "jpg",
            800.into(),
            600.into(),
        ),
        (png(31, 17, ""), "image/png", "png", 31.into(), 17.into()),
        (
            fixtures::gif(5, 7, ""),
            "image/gif",
            "gif",
            5.into(),
            7.into(),
        ),
        (
            fixtures::webp(300, 200, ""),
            "image/webp",
            "webp",
            300.into(),
            200.into(),
        ),
        (
            fixtures::avif(""),
            "image/avif",
            "avif",
            Value::Null,
            Value::Null,
        ),
        (
            fixtures::svg(""),
            "image/svg+xml",
            "svg",
            40.into(),
            30.into(),
        ),
    ];
    for (bytes, mime, ext, width, height) in cases {
        let image = app.upload_ok(&session, "file", &bytes).await;
        assert_eq!(image["type"], mime);
        assert!(
            is_default_path(image["path"].as_str().unwrap(), ext),
            "{image}"
        );
        assert_eq!(image["width"], width, "{mime}");
        assert_eq!(image["height"], height, "{mime}");
    }
}

#[tokio::test]
async fn unsupported_types_are_rejected_and_nothing_is_stored() {
    let (app, session) = app_and_session().await;
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("photo.heic", fixtures::heic("")),
        ("notes.png", b"just some text".to_vec()),
        ("page.svg", b"<html><body><svg/></body></html>".to_vec()),
        ("empty.png", Vec::new()),
        ("movie.mp4", fixtures::bmff(b"isom", &[b"mp41"], "")),
    ];
    for (name, bytes) in cases {
        let res = app.upload_as(&session, &[(name, &bytes)]).await;
        assert_eq!(res.status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{name}");
        assert_eq!(res.error_code(), "unsupported_type", "{name}");
        let message = res.json()["error"]["message"].as_str().unwrap().to_owned();
        assert!(
            message.contains("JPEG, PNG, GIF, WebP, AVIF and SVG"),
            "{message}"
        );
        let result = &res.json()["results"][0];
        assert_eq!(result["filename"], name);
        assert_eq!(result["error"]["code"], "unsupported_type");
    }
    assert_eq!(app.stored_files(), Vec::<std::path::PathBuf>::new());
}

#[tokio::test]
async fn one_bad_file_does_not_fail_the_others() {
    let (app, session) = app_and_session().await;
    let good = png(1, 1, "good");
    let bad = fixtures::heic("bad");
    let res = app
        .upload_as(&session, &[("bad.heic", &bad), ("good.png", &good)])
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    let body = res.json();
    assert_eq!(body["results"][0]["error"]["code"], "unsupported_type");
    assert!(body["results"][0]["image"].is_null());
    let path = body["results"][1]["image"]["path"].as_str().unwrap();
    assert_eq!(app.get(&format!("/{path}")).await.status, StatusCode::OK);
    assert_eq!(app.stored_files().len(), 1);
}

#[tokio::test]
async fn files_over_the_limit_are_rejected_and_nothing_is_stored() {
    let app = TestApp::builder()
        .password(PASSWORD)
        .upload_limit_mb(1)
        .build()
        .await;
    let session = app.login_ok(PASSWORD, false).await;
    let mib = 1024 * 1024;

    // Just over the limit: the request fits, the file does not.
    let mut big = png(1, 1, "");
    big.resize(mib + 1, 0);
    let res = app.upload_as(&session, &[("big.png", &big)]).await;
    assert_eq!(res.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(res.error_code(), "too_large");
    let message = res.json()["error"]["message"].as_str().unwrap().to_owned();
    assert!(message.contains("1 MB"), "{message}");

    // Far over: the request body itself is cut off.
    let mut huge = png(1, 1, "");
    huge.resize(3 * mib, 0);
    let res = app.upload_as(&session, &[("huge.png", &huge)]).await;
    assert_eq!(res.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(res.error_code(), "too_large");
    assert_eq!(app.stored_files(), Vec::<std::path::PathBuf>::new());

    // Exactly at the limit is fine.
    let mut fits = png(1, 1, "");
    fits.resize(mib, 0);
    let image = app.upload_ok(&session, "fits.png", &fits).await;
    assert_eq!(image["size"], mib);
}

#[tokio::test]
async fn stored_bytes_are_identical_to_the_uploaded_bytes() {
    let (app, session) = app_and_session().await;
    // Trailing data, odd bytes and all: nothing is re-encoded or stripped.
    let mut bytes = fixtures::jpeg(2, 2, "exif-ish");
    bytes.extend((0..=255u8).cycle().take(10_000));
    let image = app.upload_ok(&session, "x.jpg", &bytes).await;
    let served = app
        .get(&format!("/{}", image["path"].as_str().unwrap()))
        .await;
    assert_eq!(served.status, StatusCode::OK);
    assert_eq!(&served.body[..], &bytes[..]);

    // ...and they stay so across a restart.
    let restarted = app.restart().await;
    let served = restarted
        .get(&format!("/{}", image["path"].as_str().unwrap()))
        .await;
    assert_eq!(&served.body[..], &bytes[..]);
}

#[tokio::test]
async fn a_colliding_path_is_silently_redrawn() {
    let (app, session) = app_and_session().await;
    let (a, b) = (png(1, 1, "a"), png(1, 1, "b"));

    app.random.push_base62("abcdef");
    let first = app.upload_ok(&session, "a.png", &a).await;
    assert_eq!(first["path"], "abcdef.png");

    // The next draw collides with the first Image's Path, then a free one.
    app.random.push_base62("abcdef");
    app.random.push_base62("ghijkl");
    let second = app.upload_ok(&session, "b.png", &b).await;
    assert_eq!(second["path"], "ghijkl.png");

    assert_eq!(&app.get("/abcdef.png").await.body[..], &a[..]);
    assert_eq!(&app.get("/ghijkl.png").await.body[..], &b[..]);
}

#[tokio::test]
async fn a_path_under_a_reserved_prefix_is_silently_redrawn() {
    for (template, reserved) in [
        ("{random:5}/{hash:8}.{ext}", "admin"),
        ("{random:3}/{hash:8}.{ext}", "api"),
        ("{random:3}", "api"),
    ] {
        let app = TestApp::builder()
            .password(PASSWORD)
            .path_template(template)
            .build()
            .await;
        let session = app.login_ok(PASSWORD, false).await;
        app.random.push_base62(reserved);
        let bytes = png(1, 1, template);
        let image = app.upload_ok(&session, "a.png", &bytes).await;
        let path = image["path"].as_str().unwrap();
        assert!(
            !path.starts_with(reserved),
            "{template}: got reserved Path {path}"
        );
        assert_eq!(&app.get(&format!("/{path}")).await.body[..], &bytes[..]);
    }
}

#[tokio::test]
async fn uploading_the_same_bytes_again_is_a_duplicate_upload_of_the_existing_image() {
    let (app, session) = app_and_session().await;
    let bytes = png(1, 1, "same");
    let first = app.upload_ok(&session, "one.png", &bytes).await;

    app.clock.advance(Duration::hours(1));
    let res = app.upload_as(&session, &[("two.png", &bytes)]).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    let result = &res.json()["results"][0];
    assert_eq!(result["filename"], "two.png");
    assert_eq!(result["duplicate"], true);
    // The first upload's date, not now.
    assert_eq!(result["original_uploaded_at"], common::START);
    // The existing Image, unchanged: same id, record, Paths and URLs.
    assert_eq!(result["image"], first);
    assert_eq!(result["image"]["uploaded_at"], common::START);
    assert_eq!(result["image"]["original_filename"], "one.png");
}

#[tokio::test]
async fn a_duplicate_upload_lists_the_existing_paths_with_urls_and_the_primary() {
    let (app, session) = app_and_session().await;
    let bytes = png(1, 1, "paths");
    let first = app.upload_ok(&session, "a.png", &bytes).await;
    let path = first["path"].as_str().unwrap();

    let res = app.upload_as(&session, &[("a.png", &bytes)]).await;
    let image = &res.json()["results"][0]["image"];
    assert_eq!(image["path"], path);
    assert_eq!(image["url"], format!("{PUBLIC_URL}/{path}"));
    assert_eq!(
        image["paths"],
        serde_json::json!([{
            "path": path,
            "url": format!("{PUBLIC_URL}/{path}"),
            "is_primary": true,
        }])
    );
}

#[tokio::test]
async fn a_duplicate_upload_creates_no_path_and_no_file() {
    let (app, session) = app_and_session().await;
    let bytes = png(1, 1, "nothing new");
    let first = app.upload_ok(&session, "a.png", &bytes).await;
    let files = app.stored_files();
    assert_eq!(files.len(), 1);

    // Had a Path been drawn for the duplicate, it would be this one.
    app.random.push_base62("Dup123");
    let res = app.upload_as(&session, &[("b.png", &bytes)]).await;
    let image = &res.json()["results"][0]["image"];
    assert_eq!(image["paths"].as_array().unwrap().len(), 1);
    assert_eq!(image["paths"], first["paths"]);
    assert_eq!(app.get("/Dup123.png").await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.stored_files(), files);
    // The existing Path still serves the Image.
    let served = app
        .get(&format!("/{}", first["path"].as_str().unwrap()))
        .await;
    assert_eq!(&served.body[..], &bytes[..]);
}

#[tokio::test]
async fn the_same_bytes_twice_in_one_request_yield_one_image() {
    let (app, session) = app_and_session().await;
    let bytes = png(2, 2, "twice");
    let res = app
        .upload_as(&session, &[("a.png", &bytes), ("b.png", &bytes)])
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    let body = res.json();
    let (a, b) = (&body["results"][0], &body["results"][1]);
    assert_eq!(a["duplicate"], false);
    assert_eq!(b["duplicate"], true);
    assert_eq!(b["original_uploaded_at"], a["image"]["uploaded_at"]);
    assert_eq!(b["image"], a["image"]);
    assert_eq!(app.stored_files().len(), 1);
}

#[tokio::test]
async fn a_different_file_with_the_same_filename_is_not_a_duplicate() {
    let (app, session) = app_and_session().await;
    let first = app
        .upload_ok(&session, "photo.png", &png(1, 1, "one"))
        .await;
    let res = app
        .upload_as(&session, &[("photo.png", &png(1, 1, "two"))])
        .await;
    let result = &res.json()["results"][0];
    assert_eq!(result["duplicate"], false);
    assert!(result["original_uploaded_at"].is_null());
    assert_ne!(result["image"]["id"], first["id"]);
    assert_ne!(result["image"]["sha256"], first["sha256"]);
    assert_ne!(result["image"]["path"], first["path"]);
    assert_eq!(app.stored_files().len(), 2);
}

#[tokio::test]
async fn upload_requires_the_owner() {
    let app = TestApp::with_password(PASSWORD).await;
    let (content_type, body) = common::multipart_body(&[("a.png", &png(1, 1, ""))]);
    let res = app
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/images")
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .unwrap(),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    assert_eq!(res.error_code(), "unauthorized");
    assert!(app.stored_files().is_empty());
}

#[tokio::test]
async fn an_upload_without_files_is_a_bad_request() {
    let (app, session) = app_and_session().await;
    let res = app.upload_as(&session, &[]).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_eq!(res.error_code(), "bad_request");

    let res = app
        .request(
            Method::POST,
            "/api/v1/images",
            Some(&session),
            Some(serde_json::json!({ "file": "nope" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_eq!(res.error_code(), "bad_request");
}
