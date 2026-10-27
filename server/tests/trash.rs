//! The Trash: deleting (single and bulk), Restore, Permanent Delete, the
//! automatic purge after 30 days, and Duplicate Uploads of trashed Images.

mod common;

use axum::http::{Method, StatusCode};
use common::fixtures::png;
use common::{START, Session, TestApp, TestResponse};
use serde_json::{Value, json};
use time::Duration;

const PASSWORD: &str = "correct horse battery";
const DAY: i64 = 24 * 60 * 60;
const THIRTY_DAYS: i64 = 30 * DAY;

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

/// Moves the clock forward and logs in again (sessions last a day).
async fn later(app: &TestApp, by: Duration) -> Session {
    app.clock.advance(by);
    app.login_ok(PASSWORD, false).await
}

fn id(image: &Value) -> i64 {
    image["id"].as_i64().unwrap()
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

fn ids(images: &Value) -> Vec<i64> {
    images.as_array().unwrap().iter().map(id).collect()
}

async fn post(app: &TestApp, session: &Session, uri: &str, body: Value) -> TestResponse {
    app.request(Method::POST, uri, Some(session), Some(body))
        .await
}

async fn add_path(app: &TestApp, session: &Session, image: i64, path: &str) -> TestResponse {
    post(
        app,
        session,
        &format!("/api/v1/images/{image}/paths"),
        json!({ "path": path }),
    )
    .await
}

async fn add_path_ok(app: &TestApp, session: &Session, image: i64, path: &str) -> Value {
    let res = add_path(app, session, image, path).await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.text());
    res.json()
}

/// Moves one Image to the Trash with `DELETE /api/v1/images/{id}`.
async fn trash_one(app: &TestApp, session: &Session, image: i64) -> Value {
    let res = app
        .delete_as(&format!("/api/v1/images/{image}"), session)
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    res.json()
}

async fn trash_many(app: &TestApp, session: &Session, images: &[i64]) -> TestResponse {
    post(app, session, "/api/v1/trash", json!({ "ids": images })).await
}

async fn restore(app: &TestApp, session: &Session, images: &[i64]) -> TestResponse {
    post(
        app,
        session,
        "/api/v1/trash/restore",
        json!({ "ids": images }),
    )
    .await
}

async fn delete_permanently(app: &TestApp, session: &Session, images: &[i64]) -> TestResponse {
    post(
        app,
        session,
        "/api/v1/trash/delete",
        json!({ "ids": images }),
    )
    .await
}

async fn detail(app: &TestApp, session: &Session, image: i64) -> TestResponse {
    app.get_as(&format!("/api/v1/images/{image}"), session)
        .await
}

async fn library(app: &TestApp, session: &Session, query: &str) -> Value {
    let res = app
        .get_as(&format!("/api/v1/images?{query}"), session)
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    res.json()
}

async fn trash_list(app: &TestApp, session: &Session) -> Value {
    let res = app.get_as("/api/v1/trash", session).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    res.json()
}

async fn status_of(app: &TestApp, path: &str) -> StatusCode {
    app.get(&format!("/{path}")).await.status
}

#[tokio::test]
async fn deleting_one_image_moves_it_to_the_trash_and_its_paths_stop_serving() {
    let (app, session) = app_and_session().await;
    let cat = upload(&app, &session, "Cat111", "cat").await;
    add_path_ok(&app, &session, id(&cat), "cat.png").await;
    let dog = upload(&app, &session, "Dog222", "dog").await;
    let size = cat["size"].as_i64().unwrap() + dog["size"].as_i64().unwrap();
    let session = later(&app, Duration::hours(5)).await;

    let trashed = trash_one(&app, &session, id(&cat)).await;
    let now = START + 5 * 60 * 60;
    assert_eq!(trashed["trashed_at"], now);
    assert_eq!(trashed["permanent_delete_at"], now + THIRTY_DAYS);
    assert_eq!(paths(&trashed), ["Cat111.png*", "cat.png"]);

    // Every Path of the trashed Image is 404 to Visitors; the other Image
    // still serves.
    assert_eq!(status_of(&app, "Cat111.png").await, StatusCode::NOT_FOUND);
    assert_eq!(status_of(&app, "cat.png").await, StatusCode::NOT_FOUND);
    let head = app.request(Method::HEAD, "/cat.png", None, None).await;
    assert_eq!(head.status, StatusCode::NOT_FOUND);
    assert_eq!(status_of(&app, "Dog222.png").await, StatusCode::OK);

    // Gone from the Library and its search, still found by id.
    let list = library(&app, &session, "").await;
    assert_eq!(ids(&list["images"]), [id(&dog)]);
    assert_eq!(list["total"], 1);
    assert_eq!(library(&app, &session, "q=Cat111").await["total"], 0);
    assert_eq!(library(&app, &session, "q=cat").await["total"], 0);
    let res = detail(&app, &session, id(&cat)).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.json()["trashed_at"], now);
    assert_eq!(res.json()["permanent_delete_at"], now + THIRTY_DAYS);
    assert!(detail(&app, &session, id(&dog)).await.json()["permanent_delete_at"].is_null());

    // The Owner can still preview it.
    let res = app
        .get_as(&format!("/api/v1/images/{}/file", id(&cat)), &session)
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body.as_ref(), png(4, 4, "cat").as_slice());
    assert_eq!(res.header("content-type"), Some("image/png"));
    assert_eq!(
        res.header("content-security-policy"),
        Some(img_bed::serve::CONTENT_SECURITY_POLICY)
    );
    assert_eq!(res.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(res.header("cache-control"), Some("private, no-cache"));
    let res = app.get_as("/api/v1/images/9999/file", &session).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);

    // The count leaves the Trash out; its files still use the disk.
    assert_eq!(list["stats"]["image_count"], 1);
    assert_eq!(list["stats"]["disk_usage"], size);
    let info = app.get_as("/api/v1/info", &session).await.json();
    assert_eq!(info["image_count"], 1);
    assert_eq!(info["disk_usage"], size);
}

#[tokio::test]
async fn bulk_delete_moves_the_selection_and_the_library_gives_the_path_counts() {
    let (app, session) = app_and_session().await;
    let a = upload(&app, &session, "Aaa111", "a").await;
    add_path_ok(&app, &session, id(&a), "a-2.png").await;
    add_path_ok(&app, &session, id(&a), "a/3.png").await;
    let b = upload(&app, &session, "Bbb222", "b").await;
    add_path_ok(&app, &session, id(&b), "b-2.png").await;
    let c = upload(&app, &session, "Ccc333", "c").await;

    // What the confirmation counts: the selected Images and all their Paths,
    // as the Library lists them.
    let list = library(&app, &session, "").await;
    let selected = [id(&a), id(&b)];
    let path_count: usize = list["images"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|image| selected.contains(&id(image)))
        .map(|image| image["paths"].as_array().unwrap().len())
        .sum();
    assert_eq!(path_count, 5);

    let res = trash_many(&app, &session, &selected).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    let trashed = res.json()["images"].clone();
    assert_eq!(ids(&trashed), selected);
    let trashed_paths: Vec<String> = trashed
        .as_array()
        .unwrap()
        .iter()
        .flat_map(paths)
        .map(|p| p.trim_end_matches('*').to_owned())
        .collect();
    assert_eq!(trashed_paths.len(), path_count);
    for path in &trashed_paths {
        assert_eq!(
            status_of(&app, path).await,
            StatusCode::NOT_FOUND,
            "/{path}"
        );
    }
    assert_eq!(status_of(&app, "Ccc333.png").await, StatusCode::OK);
    assert_eq!(ids(&library(&app, &session, "").await["images"]), [id(&c)]);
    assert_eq!(trash_list(&app, &session).await["total"], 2);
}

#[tokio::test]
async fn bulk_delete_is_all_or_nothing_and_checks_its_input() {
    let (app, session) = app_and_session().await;
    let a = upload(&app, &session, "Aaa111", "a").await;

    let res = trash_many(&app, &session, &[id(&a), 9999]).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert_eq!(status_of(&app, "Aaa111.png").await, StatusCode::OK);
    assert_eq!(trash_list(&app, &session).await["total"], 0);

    let res = trash_many(&app, &session, &[]).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    for body in [
        json!({}),
        json!({ "ids": "1" }),
        json!({ "ids": [1], "x": 1 }),
    ] {
        let res = post(&app, &session, "/api/v1/trash", body.clone()).await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(res.error_code(), "bad_request");
    }
    let res = app.delete_as("/api/v1/images/9999", &session).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    let res = app.delete_as("/api/v1/images/abc", &session).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);

    // Duplicate ids count once; deleting again keeps the first date.
    let res = trash_many(&app, &session, &[id(&a), id(&a)]).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    assert_eq!(ids(&res.json()["images"]), [id(&a)]);
    let session = later(&app, Duration::days(3)).await;
    let again = trash_one(&app, &session, id(&a)).await;
    assert_eq!(again["trashed_at"], START);
}

#[tokio::test]
async fn a_path_held_by_an_image_in_trash_cannot_be_added() {
    let (app, session) = app_and_session().await;
    let a = upload(&app, &session, "Aaa111", "a").await;
    add_path_ok(&app, &session, id(&a), "kept.png").await;
    let b = upload(&app, &session, "Bbb222", "b").await;
    trash_one(&app, &session, id(&a)).await;

    for path in ["kept.png", "Aaa111.png"] {
        let res = add_path(&app, &session, id(&b), path).await;
        assert_eq!(res.status, StatusCode::CONFLICT);
        assert_eq!(res.error_code(), "path_in_trash");
        let message = res.json()["error"]["message"].as_str().unwrap().to_owned();
        assert!(
            message.contains(&format!("/{path} is held by an Image in Trash")),
            "{message}"
        );
    }
    // A live holder is still reported as before.
    let c = upload(&app, &session, "Ccc333", "c").await;
    let res = add_path(&app, &session, id(&c), "Bbb222.png").await;
    assert_eq!(res.error_code(), "path_taken");

    // The Path Template skips Paths held in the Trash too.
    app.random.push_base62("Aaa111");
    app.random.push_base62("Ddd444");
    let d = app.upload_ok(&session, "d.png", &png(4, 4, "d")).await;
    assert_eq!(d["path"], "Ddd444.png");
}

#[tokio::test]
async fn the_paths_of_a_trashed_image_cannot_be_changed_until_it_is_restored() {
    let (app, session) = app_and_session().await;
    let a = upload(&app, &session, "Aaa111", "a").await;
    add_path_ok(&app, &session, id(&a), "second.png").await;
    trash_one(&app, &session, id(&a)).await;

    let res = add_path(&app, &session, id(&a), "third.png").await;
    assert_eq!(res.status, StatusCode::CONFLICT);
    assert_eq!(res.error_code(), "image_in_trash");
    let res = app
        .delete_as(
            &format!("/api/v1/images/{}/paths/second.png", id(&a)),
            &session,
        )
        .await;
    assert_eq!(res.error_code(), "image_in_trash");
    let res = app
        .request(
            Method::PUT,
            &format!("/api/v1/images/{}/primary", id(&a)),
            Some(&session),
            Some(json!({ "path": "second.png" })),
        )
        .await;
    assert_eq!(res.error_code(), "image_in_trash");

    // The refused Path stayed free, the others unchanged.
    let b = upload(&app, &session, "Bbb222", "b").await;
    add_path_ok(&app, &session, id(&b), "third.png").await;
    let image = detail(&app, &session, id(&a)).await.json();
    assert_eq!(paths(&image), ["Aaa111.png*", "second.png"]);
}

#[tokio::test]
async fn restore_brings_back_every_path_exactly() {
    let (app, session) = app_and_session().await;
    let a = upload(&app, &session, "Aaa111", "a").await;
    add_path_ok(&app, &session, id(&a), "old/link.png").await;
    add_path_ok(&app, &session, id(&a), "截图.png").await;
    let res = app
        .request(
            Method::PUT,
            &format!("/api/v1/images/{}/primary", id(&a)),
            Some(&session),
            Some(json!({ "path": "old/link.png" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let before = detail(&app, &session, id(&a)).await.json();
    let b = upload(&app, &session, "Bbb222", "b").await;

    trash_many(&app, &session, &[id(&a), id(&b)]).await;
    let session = later(&app, Duration::days(29)).await;
    let res = restore(&app, &session, &[id(&a)]).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    let restored = &res.json()["images"][0];
    assert_eq!(paths(restored), paths(&before));
    assert_eq!(paths(restored), ["old/link.png*", "Aaa111.png", "截图.png"]);
    assert!(restored["trashed_at"].is_null());
    assert!(restored["permanent_delete_at"].is_null());

    let bytes = png(4, 4, "a");
    for path in ["Aaa111.png", "old/link.png", "%E6%88%AA%E5%9B%BE.png"] {
        let res = app.get(&format!("/{path}")).await;
        assert_eq!(res.status, StatusCode::OK, "/{path}");
        assert_eq!(res.body.as_ref(), bytes.as_slice());
    }
    // Back in the Library at its original place; out of the Trash; and the
    // automatic purge no longer applies to it.
    let list = library(&app, &session, "").await;
    assert_eq!(ids(&list["images"]), [id(&a)]);
    assert_eq!(list["stats"]["image_count"], 1);
    assert_eq!(ids(&trash_list(&app, &session).await["images"]), [id(&b)]);
    let session = later(&app, Duration::days(2)).await;
    assert_eq!(app.purge_trash().await, 1);
    assert_eq!(status_of(&app, "Aaa111.png").await, StatusCode::OK);

    // Restoring an Image that is not in the Trash changes nothing; an
    // unknown (here: Permanently Deleted) one is 404 and nothing happens.
    let res = restore(&app, &session, &[id(&a)]).await;
    assert_eq!(res.status, StatusCode::OK);
    let c = upload(&app, &session, "Ccc333", "c").await;
    trash_one(&app, &session, id(&c)).await;
    let res = restore(&app, &session, &[id(&c), id(&b)]).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert_eq!(status_of(&app, "Ccc333.png").await, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_trash_lists_images_with_their_permanent_delete_dates() {
    let (app, session) = app_and_session().await;
    let a = upload(&app, &session, "Aaa111", "a").await;
    let b = upload(&app, &session, "Bbb222", "b").await;
    let c = upload(&app, &session, "Ccc333", "c").await;
    trash_one(&app, &session, id(&b)).await;
    let session = later(&app, Duration::days(2)).await;
    trash_one(&app, &session, id(&a)).await;

    let list = trash_list(&app, &session).await;
    assert_eq!(list["total"], 2);
    assert_eq!(list["pages"], 1);
    assert_eq!(list["trash_days"], 30);
    // Most recently trashed first.
    assert_eq!(ids(&list["images"]), [id(&a), id(&b)]);
    let first = &list["images"][0];
    assert_eq!(first["trashed_at"], START + 2 * DAY);
    assert_eq!(first["permanent_delete_at"], START + 2 * DAY + THIRTY_DAYS);
    assert_eq!(first["path"], "Aaa111.png");
    assert_eq!(
        list["images"][1]["permanent_delete_at"],
        START + THIRTY_DAYS
    );

    let page = app
        .get_as("/api/v1/trash?per_page=1&page=2", &session)
        .await
        .json();
    assert_eq!(ids(&page["images"]), [id(&b)]);
    assert_eq!(page["pages"], 2);
    let res = app.get_as("/api/v1/trash?page=0", &session).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);

    assert_eq!(ids(&library(&app, &session, "").await["images"]), [id(&c)]);
}

#[tokio::test]
async fn permanent_delete_erases_the_file_and_record_and_frees_the_paths() {
    let (app, session) = app_and_session().await;
    let a = upload(&app, &session, "Aaa111", "a").await;
    add_path_ok(&app, &session, id(&a), "taken.png").await;
    let b = upload(&app, &session, "Bbb222", "b").await;
    assert_eq!(app.stored_files().len(), 2);

    // Only Images in the Trash can be Permanently Deleted.
    let res = delete_permanently(&app, &session, &[id(&a)]).await;
    assert_eq!(res.status, StatusCode::CONFLICT);
    assert_eq!(res.error_code(), "not_in_trash");
    trash_one(&app, &session, id(&a)).await;
    let res = delete_permanently(&app, &session, &[id(&a), id(&b)]).await;
    assert_eq!(res.error_code(), "not_in_trash");
    let res = delete_permanently(&app, &session, &[id(&a), 9999]).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert_eq!(app.stored_files().len(), 2, "nothing was deleted");
    assert_eq!(detail(&app, &session, id(&a)).await.status, StatusCode::OK);

    let res = delete_permanently(&app, &session, &[id(&a)]).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    assert_eq!(res.json()["deleted"], 1);
    assert_eq!(app.stored_files().len(), 1, "the file is erased");
    assert_eq!(
        detail(&app, &session, id(&a)).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(trash_list(&app, &session).await["total"], 0);
    assert_eq!(status_of(&app, "taken.png").await, StatusCode::NOT_FOUND);
    let stats = library(&app, &session, "").await["stats"].clone();
    assert_eq!(stats["disk_usage"], b["size"]);

    // Its Paths can now go to another Image.
    add_path_ok(&app, &session, id(&b), "taken.png").await;
    add_path_ok(&app, &session, id(&b), "Aaa111.png").await;
    assert_eq!(status_of(&app, "Aaa111.png").await, StatusCode::OK);

    // Uploading the same bytes again makes a new Image with a new file.
    let again = upload(&app, &session, "New111", "a").await;
    assert_ne!(id(&again), id(&a));
    assert_eq!(app.stored_files().len(), 2);
    assert_eq!(status_of(&app, "New111.png").await, StatusCode::OK);

    // Twice is 404.
    let res = delete_permanently(&app, &session, &[id(&a)]).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn permanent_delete_of_one_image_and_of_a_missing_file() {
    let (app, session) = app_and_session().await;
    let a = upload(&app, &session, "Aaa111", "a").await;
    let b = upload(&app, &session, "Bbb222", "b").await;
    trash_many(&app, &session, &[id(&a), id(&b)]).await;

    let res = app
        .delete_as(&format!("/api/v1/trash/{}", id(&a)), &session)
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    assert_eq!(res.json()["deleted"], 1);

    // A file already gone from disk doesn't stop the record going.
    for file in app.stored_files() {
        std::fs::remove_file(file).unwrap();
    }
    let res = app
        .delete_as(&format!("/api/v1/trash/{}", id(&b)), &session)
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    assert_eq!(
        detail(&app, &session, id(&b)).await.status,
        StatusCode::NOT_FOUND
    );

    // A later upload never gets a Permanently Deleted Image's id.
    let c = upload(&app, &session, "Ccc333", "c").await;
    assert!(id(&c) > id(&b));
    assert_eq!(
        detail(&app, &session, id(&b)).await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn images_in_the_trash_over_30_days_are_permanently_deleted_automatically() {
    let (app, session) = app_and_session().await;
    let a = upload(&app, &session, "Aaa111", "a").await;
    let b = upload(&app, &session, "Bbb222", "b").await;
    let live = upload(&app, &session, "Live11", "live").await;
    trash_one(&app, &session, id(&a)).await;
    let session = later(&app, Duration::days(10)).await;
    trash_one(&app, &session, id(&b)).await;

    // Nothing is due a second before the first one's date.
    let session = later(&app, Duration::days(20) - Duration::seconds(1)).await;
    assert_eq!(app.purge_trash().await, 0);
    assert_eq!(trash_list(&app, &session).await["total"], 2);

    let session = later(&app, Duration::seconds(1)).await;
    assert_eq!(app.purge_trash().await, 1);
    assert_eq!(
        detail(&app, &session, id(&a)).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(ids(&trash_list(&app, &session).await["images"]), [id(&b)]);
    assert_eq!(app.stored_files().len(), 2);
    add_path_ok(&app, &session, id(&live), "Aaa111.png").await;

    let session = later(&app, Duration::days(11)).await;
    assert_eq!(app.purge_trash().await, 1);
    assert_eq!(trash_list(&app, &session).await["total"], 0);
    assert_eq!(app.stored_files().len(), 1);
    assert_eq!(status_of(&app, "Live11.png").await, StatusCode::OK);
    assert_eq!(app.purge_trash().await, 0);
}

#[tokio::test]
async fn uploading_the_bytes_of_a_trashed_image_restores_it() {
    let (app, session) = app_and_session().await;
    let a = upload(&app, &session, "Aaa111", "a").await;
    add_path_ok(&app, &session, id(&a), "second.png").await;
    trash_one(&app, &session, id(&a)).await;
    let session = later(&app, Duration::days(3)).await;

    let res = app
        .upload_as(&session, &[("again.png", &png(4, 4, "a"))])
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    let result = &res.json()["results"][0];
    assert_eq!(result["duplicate"], true);
    assert_eq!(result["restored"], true);
    assert_eq!(result["original_uploaded_at"], START);
    assert_eq!(id(&result["image"]), id(&a));
    assert!(result["image"]["trashed_at"].is_null());
    assert_eq!(paths(&result["image"]), ["Aaa111.png*", "second.png"]);
    for path in ["Aaa111.png", "second.png"] {
        assert_eq!(status_of(&app, path).await, StatusCode::OK, "/{path}");
    }
    assert_eq!(trash_list(&app, &session).await["total"], 0);
    assert_eq!(ids(&library(&app, &session, "").await["images"]), [id(&a)]);
    assert_eq!(app.stored_files().len(), 1);

    // Once restored, a further Duplicate Upload is a plain one.
    let res = app
        .upload_as(&session, &[("again.png", &png(4, 4, "a"))])
        .await;
    let result = &res.json()["results"][0];
    assert_eq!(result["duplicate"], true);
    assert_eq!(result["restored"], false);
    // And a new Image is neither.
    let res = app
        .upload_as(&session, &[("new.png", &png(4, 4, "new"))])
        .await;
    let result = &res.json()["results"][0];
    assert_eq!(result["duplicate"], false);
    assert_eq!(result["restored"], false);
}

#[tokio::test]
async fn trash_endpoints_require_a_session() {
    let (app, session) = app_and_session().await;
    let a = upload(&app, &session, "Aaa111", "a").await;
    let ids = json!({ "ids": [id(&a)] });
    let image = format!("/api/v1/images/{}", id(&a));
    let trash_item = format!("/api/v1/trash/{}", id(&a));
    let file = format!("/api/v1/images/{}/file", id(&a));
    let requests: [(Method, &str, Option<Value>); 7] = [
        (Method::DELETE, &image, None),
        (Method::GET, &file, None),
        (Method::POST, "/api/v1/trash", Some(ids.clone())),
        (Method::GET, "/api/v1/trash", None),
        (Method::POST, "/api/v1/trash/restore", Some(ids.clone())),
        (Method::POST, "/api/v1/trash/delete", Some(ids.clone())),
        (Method::DELETE, &trash_item, None),
    ];
    for (method, uri, body) in requests {
        let res = app.request(method.clone(), uri, None, body).await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED, "{method} {uri}");
        assert_eq!(res.error_code(), "unauthorized");
    }
    assert_eq!(status_of(&app, "Aaa111.png").await, StatusCode::OK);
}
