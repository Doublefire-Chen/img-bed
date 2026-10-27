//! The Library: listing order, paging, search, detail and totals.

mod common;

use axum::http::StatusCode;
use common::fixtures::{gif, jpeg, png};
use common::{PUBLIC_URL, Session, TestApp};
use serde_json::{Value, json};
use time::Duration;

const PASSWORD: &str = "correct horse battery";

async fn app_and_session() -> (TestApp, Session) {
    let app = TestApp::with_password(PASSWORD).await;
    // Remembered, so tests can move the clock on by days.
    let session = app.login_ok(PASSWORD, true).await;
    (app, session)
}

/// `GET /api/v1/images<query>` as the Owner, asserting 200.
async fn library(app: &TestApp, session: &Session, query: &str) -> Value {
    let res = app.get_as(&format!("/api/v1/images{query}"), session).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    res.json()
}

fn ids(page: &Value) -> Vec<i64> {
    page["images"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_i64().unwrap())
        .collect()
}

fn id(image: &Value) -> i64 {
    image["id"].as_i64().unwrap()
}

/// Uploads `n` distinct PNGs one minute apart; returns their ids, oldest first.
async fn upload_many(app: &TestApp, session: &Session, n: usize) -> Vec<i64> {
    let mut out = Vec::new();
    for i in 0..n {
        let image = app
            .upload_ok(
                session,
                &format!("img-{i}.png"),
                &png(10, 10, &i.to_string()),
            )
            .await;
        out.push(id(&image));
        app.clock.advance(Duration::minutes(1));
    }
    out
}

#[tokio::test]
async fn an_empty_library() {
    let (app, session) = app_and_session().await;
    let page = library(&app, &session, "").await;
    assert_eq!(page["images"], json!([]));
    assert_eq!(page["total"], 0);
    assert_eq!(page["pages"], 0);
    assert_eq!(page["page"], 1);
    assert_eq!(page["stats"], json!({ "image_count": 0, "disk_usage": 0 }));
}

#[tokio::test]
async fn the_library_lists_images_newest_first() {
    let (app, session) = app_and_session().await;
    let first = app.upload_ok(&session, "first.png", &png(1, 1, "1")).await;
    app.clock.advance(Duration::hours(1));
    let second = app.upload_ok(&session, "second.gif", &gif(2, 2, "2")).await;
    app.clock.advance(Duration::days(3));
    let third = app.upload_ok(&session, "third.jpg", &jpeg(3, 3, "3")).await;

    let page = library(&app, &session, "").await;
    assert_eq!(ids(&page), [id(&third), id(&second), id(&first)]);
    let uploaded: Vec<_> = page["images"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["uploaded_at"].as_i64().unwrap())
        .collect();
    assert_eq!(
        uploaded,
        [
            common::START + 3600 + 3 * 86400,
            common::START + 3600,
            common::START
        ]
    );
}

#[tokio::test]
async fn images_uploaded_in_the_same_second_are_ordered_by_upload_order() {
    let (app, session) = app_and_session().await;
    let a = app.upload_ok(&session, "a.png", &png(1, 1, "a")).await;
    let b = app.upload_ok(&session, "b.png", &png(1, 1, "b")).await;
    let c = app.upload_ok(&session, "c.png", &png(1, 1, "c")).await;
    let page = library(&app, &session, "").await;
    assert_eq!(ids(&page), [id(&c), id(&b), id(&a)]);
    // And the order is stable across pages.
    let p1 = library(&app, &session, "?per_page=2&page=1").await;
    let p2 = library(&app, &session, "?per_page=2&page=2").await;
    assert_eq!(ids(&p1), [id(&c), id(&b)]);
    assert_eq!(ids(&p2), [id(&a)]);
}

#[tokio::test]
async fn a_duplicate_upload_keeps_the_original_place_in_the_order() {
    let (app, session) = app_and_session().await;
    let old = app.upload_ok(&session, "old.png", &png(1, 1, "old")).await;
    app.clock.advance(Duration::hours(1));
    let new = app.upload_ok(&session, "new.png", &png(1, 1, "new")).await;
    app.clock.advance(Duration::hours(1));
    app.upload_ok(&session, "old again.png", &png(1, 1, "old"))
        .await;
    let page = library(&app, &session, "").await;
    assert_eq!(ids(&page), [id(&new), id(&old)]);
}

#[tokio::test]
async fn the_library_pages_correctly() {
    let (app, session) = app_and_session().await;
    let mut uploaded = upload_many(&app, &session, 5).await;
    uploaded.reverse(); // newest first

    let p1 = library(&app, &session, "?per_page=2").await;
    let p2 = library(&app, &session, "?per_page=2&page=2").await;
    let p3 = library(&app, &session, "?per_page=2&page=3").await;
    let p4 = library(&app, &session, "?per_page=2&page=4").await;
    for (n, p) in [&p1, &p2, &p3, &p4].into_iter().enumerate() {
        assert_eq!(p["page"], n + 1);
        assert_eq!(p["per_page"], 2);
        assert_eq!(p["total"], 5);
        assert_eq!(p["pages"], 3);
    }
    assert_eq!(ids(&p1), uploaded[0..2]);
    assert_eq!(ids(&p2), uploaded[2..4]);
    assert_eq!(ids(&p3), uploaded[4..5]);
    assert_eq!(ids(&p4), Vec::<i64>::new());

    let all = library(&app, &session, "?per_page=5").await;
    assert_eq!(ids(&all), uploaded);
    assert_eq!(all["pages"], 1);
}

#[tokio::test]
async fn the_default_page_size_is_used_without_per_page() {
    let (app, session) = app_and_session().await;
    upload_many(&app, &session, 3).await;
    let page = library(&app, &session, "").await;
    assert_eq!(page["per_page"], 60);
    assert_eq!(page["pages"], 1);
    assert_eq!(ids(&page).len(), 3);
}

#[tokio::test]
async fn invalid_paging_is_a_bad_request() {
    let (app, session) = app_and_session().await;
    for query in [
        "?page=0",
        "?page=-1",
        "?page=x",
        "?per_page=0",
        "?per_page=201",
        "?per_page=two",
    ] {
        let res = app
            .get_as(&format!("/api/v1/images{query}"), &session)
            .await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "{query}");
        assert_eq!(res.error_code(), "bad_request", "{query}");
    }
    assert_eq!(
        library(&app, &session, "?per_page=200").await["per_page"],
        200
    );
}

#[tokio::test]
async fn list_entries_carry_the_primary_path_url_that_serves_the_thumbnail() {
    let (app, session) = app_and_session().await;
    let bytes = png(640, 480, "x");
    app.random.push_base62("Thumb1");
    app.upload_ok(&session, "Holiday.png", &bytes).await;

    let page = library(&app, &session, "").await;
    let image = &page["images"][0];
    assert_eq!(image["path"], "Thumb1.png");
    assert_eq!(image["url"], format!("{PUBLIC_URL}/Thumb1.png"));
    assert_eq!(image["original_filename"], "Holiday.png");
    assert_eq!(image["width"], 640);
    assert_eq!(image["height"], 480);
    assert_eq!(image["type"], "image/png");
    assert_eq!(image["size"], bytes.len());
    assert!(image["trashed_at"].is_null());
    assert_eq!(
        image["paths"],
        json!([{ "path": "Thumb1.png", "url": format!("{PUBLIC_URL}/Thumb1.png"), "is_primary": true }])
    );

    let served = app.get("/Thumb1.png").await;
    assert_eq!(served.status, StatusCode::OK);
    assert_eq!(served.body.as_ref(), bytes.as_slice());
}

/// Three Images with known Paths and filenames, uploaded a minute apart.
/// Returns their ids: (Sunset, cat, Report).
async fn searchable(app: &TestApp, session: &Session) -> (i64, i64, i64) {
    app.random.push_base62("SunAbc");
    let sunset = app
        .upload_ok(session, "beach sunset.png", &png(1, 1, "1"))
        .await;
    app.clock.advance(Duration::minutes(1));
    app.random.push_base62("Kitty9");
    let cat = app
        .upload_ok(session, "IMG_0042.jpg", &jpeg(1, 1, "2"))
        .await;
    app.clock.advance(Duration::minutes(1));
    app.random.push_base62("Zq8Rp2");
    let report = app
        .upload_ok(session, "Quarterly Report.gif", &gif(1, 1, "3"))
        .await;
    (id(&sunset), id(&cat), id(&report))
}

async fn search(app: &TestApp, session: &Session, q: &str) -> Vec<i64> {
    let encoded: String = q
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    let page = library(app, session, &format!("?q={encoded}")).await;
    assert_eq!(page["total"], ids(&page).len(), "search {q}");
    ids(&page)
}

#[tokio::test]
async fn search_finds_images_by_path() {
    let (app, session) = app_and_session().await;
    let (sunset, cat, report) = searchable(&app, &session).await;
    assert_eq!(search(&app, &session, "Kitty9.jpg").await, [cat]);
    assert_eq!(search(&app, &session, "itty").await, [cat]);
    assert_eq!(search(&app, &session, "SunAbc").await, [sunset]);
    assert_eq!(search(&app, &session, "Rp2.gif").await, [report]);
    // The extension is part of the Path.
    assert_eq!(search(&app, &session, ".jpg").await, [cat]);
}

#[tokio::test]
async fn search_finds_images_by_original_filename() {
    let (app, session) = app_and_session().await;
    let (sunset, cat, report) = searchable(&app, &session).await;
    assert_eq!(search(&app, &session, "beach sunset").await, [sunset]);
    assert_eq!(search(&app, &session, "IMG_0042").await, [cat]);
    assert_eq!(search(&app, &session, "Quarterly").await, [report]);
}

#[tokio::test]
async fn search_ignores_case_and_keeps_newest_first() {
    let (app, session) = app_and_session().await;
    let (sunset, cat, report) = searchable(&app, &session).await;
    assert_eq!(search(&app, &session, "kitty9").await, [cat]);
    assert_eq!(search(&app, &session, "QUARTERLY report").await, [report]);
    // "sun" is in a Path (SunAbc) and in a filename (beach sunset): one hit.
    assert_eq!(search(&app, &session, "SUN").await, [sunset]);
    // "a"/"A" is in "Quarterly Report" and in "beach sunset" / "SunAbc".
    assert_eq!(search(&app, &session, "a").await, [report, sunset]);
}

#[tokio::test]
async fn search_with_no_match_is_empty_and_blank_search_lists_everything() {
    let (app, session) = app_and_session().await;
    let (sunset, cat, report) = searchable(&app, &session).await;
    let page = library(&app, &session, "?q=nothing-like-this").await;
    assert_eq!(page["images"], json!([]));
    assert_eq!(page["total"], 0);
    assert_eq!(page["pages"], 0);
    // The totals don't depend on the search.
    assert_eq!(page["stats"]["image_count"], 3);
    assert_eq!(search(&app, &session, "").await, [report, cat, sunset]);
    assert_eq!(search(&app, &session, "   ").await, [report, cat, sunset]);
    // Surrounding blanks are ignored.
    assert_eq!(search(&app, &session, "  Kitty9 ").await, [cat]);
}

#[tokio::test]
async fn search_treats_like_wildcards_literally() {
    let (app, session) = app_and_session().await;
    let percent = app.upload_ok(&session, "100%.png", &png(1, 1, "p")).await;
    let plain = app.upload_ok(&session, "1000.png", &png(1, 1, "q")).await;
    let underscore = app.upload_ok(&session, "a_b.png", &png(1, 1, "u")).await;
    let ab = app.upload_ok(&session, "axb.png", &png(1, 1, "v")).await;
    let backslash = app.upload_ok(&session, "c\\d.png", &png(1, 1, "w")).await;

    assert_eq!(search(&app, &session, "100%").await, [id(&percent)]);
    assert_eq!(search(&app, &session, "%").await, [id(&percent)]);
    assert_eq!(search(&app, &session, "a_b").await, [id(&underscore)]);
    assert_eq!(search(&app, &session, "_").await, [id(&underscore)]);
    assert_eq!(search(&app, &session, "c\\d").await, [id(&backslash)]);
    assert_eq!(
        search(&app, &session, "100").await,
        [id(&plain), id(&percent)]
    );
    assert_eq!(search(&app, &session, "axb").await, [id(&ab)]);
}

#[tokio::test]
async fn search_results_are_paged() {
    let (app, session) = app_and_session().await;
    let mut cats = Vec::new();
    for i in 0..3 {
        cats.push(id(&app
            .upload_ok(
                &session,
                &format!("cat {i}.png"),
                &png(1, 1, &format!("c{i}")),
            )
            .await));
        app.upload_ok(
            &session,
            &format!("dog {i}.png"),
            &png(1, 1, &format!("d{i}")),
        )
        .await;
        app.clock.advance(Duration::minutes(1));
    }
    cats.reverse();
    let p1 = library(&app, &session, "?q=cat&per_page=2").await;
    let p2 = library(&app, &session, "?q=cat&per_page=2&page=2").await;
    assert_eq!(p1["total"], 3);
    assert_eq!(p1["pages"], 2);
    assert_eq!(ids(&p1), cats[0..2]);
    assert_eq!(ids(&p2), cats[2..3]);
}

#[tokio::test]
async fn the_detail_view_shows_everything_about_an_image() {
    let (app, session) = app_and_session().await;
    app.clock.advance(Duration::days(5));
    let bytes = jpeg(800, 600, "detail");
    app.random.push_base62("Detail");
    let uploaded = app.upload_ok(&session, "Mountain View.jpg", &bytes).await;

    let res = app
        .get_as(&format!("/api/v1/images/{}", id(&uploaded)), &session)
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text());
    let image = res.json();
    assert_eq!(image["id"], uploaded["id"]);
    assert_eq!(image["size"], bytes.len());
    assert_eq!(image["width"], 800);
    assert_eq!(image["height"], 600);
    assert_eq!(image["type"], "image/jpeg");
    assert_eq!(image["sha256"], uploaded["sha256"]);
    assert_eq!(image["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(image["uploaded_at"], common::START + 5 * 86400);
    assert_eq!(image["original_filename"], "Mountain View.jpg");
    assert!(image["trashed_at"].is_null());
    // The preview is the Primary Path's URL.
    assert_eq!(image["path"], "Detail.jpg");
    assert_eq!(image["url"], format!("{PUBLIC_URL}/Detail.jpg"));
    assert_eq!(
        image["paths"],
        json!([{ "path": "Detail.jpg", "url": format!("{PUBLIC_URL}/Detail.jpg"), "is_primary": true }])
    );
    // Same shape as the Library entry.
    let page = library(&app, &session, "").await;
    assert_eq!(page["images"][0], image);
}

#[tokio::test]
async fn an_unknown_image_is_not_found() {
    let (app, session) = app_and_session().await;
    for uri in [
        "/api/v1/images/1",
        "/api/v1/images/999999",
        "/api/v1/images/abc",
        "/api/v1/images/-1",
    ] {
        let res = app.get_as(uri, &session).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(res.error_code(), "not_found", "{uri}");
    }
}

#[tokio::test]
async fn count_and_disk_usage_are_correct() {
    let (app, session) = app_and_session().await;
    let (a, b, c) = (png(10, 10, "a"), gif(5, 5, "bb"), jpeg(7, 7, "ccc"));
    app.upload_ok(&session, "a.png", &a).await;
    app.upload_ok(&session, "b.gif", &b).await;
    app.upload_ok(&session, "c.jpg", &c).await;
    // A Duplicate Upload adds nothing.
    app.upload_ok(&session, "a again.png", &a).await;
    let expected = json!({
        "image_count": 3,
        "disk_usage": a.len() + b.len() + c.len(),
    });

    let page = library(&app, &session, "?per_page=1&page=2").await;
    assert_eq!(page["stats"], expected);

    // The Owner also gets them from info.
    let info = app.get_as("/api/v1/info", &session).await.json();
    assert_eq!(info["image_count"], expected["image_count"]);
    assert_eq!(info["disk_usage"], expected["disk_usage"]);
    assert!(info["version"].is_string());
}

#[tokio::test]
async fn visitors_get_only_the_version_from_info() {
    let (app, session) = app_and_session().await;
    app.upload_ok(&session, "a.png", &png(1, 1, "a")).await;
    let info = app.get("/api/v1/info").await;
    assert_eq!(info.status, StatusCode::OK);
    let body = info.json();
    assert_eq!(body.as_object().unwrap().len(), 1, "{body}");
    assert!(body["version"].is_string());
}

#[tokio::test]
async fn library_endpoints_require_the_owner() {
    let (app, session) = app_and_session().await;
    let image = app.upload_ok(&session, "a.png", &png(1, 1, "a")).await;
    for uri in [
        "/api/v1/images".to_owned(),
        "/api/v1/images?page=2&q=a".to_owned(),
        "/api/v1/images?page=0".to_owned(),
        format!("/api/v1/images/{}", id(&image)),
        "/api/v1/images/999".to_owned(),
        "/api/v1/images/abc".to_owned(),
    ] {
        let res = app.get(&uri).await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED, "{uri}");
        assert_eq!(res.error_code(), "unauthorized", "{uri}");
        let stale = Session {
            token: "not-a-real-session".into(),
        };
        let res = app.get_as(&uri, &stale).await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED, "{uri}");
    }
}
