//! Shared test harness: the whole app built in-process against a temporary
//! data directory and a fresh SQLite database, driven over HTTP with
//! `tower::ServiceExt::oneshot` (no network port), with a [`ManualClock`]
//! the test controls.
//!
//! ```ignore
//! mod common;
//! use common::TestApp;
//!
//! let app = TestApp::with_password("correct horse").await;
//! let session = app.login_ok("correct horse", false).await;
//! let res = app.get_as("/api/v1/session", &session).await;
//! assert_eq!(res.status, StatusCode::OK);
//! app.clock.advance(Duration::hours(24));
//! ```
//!
//! Tests only look at requests and responses (and CLI output), never at
//! database rows.

#![allow(dead_code)] // each test binary uses a different subset

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Arc;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use img_bed::app;
use img_bed::auth::SESSION_COOKIE;
use img_bed::clock::ManualClock;
use img_bed::config::Config;
use img_bed::random::SeededRandom;
use img_bed::state::AppState;
use serde_json::Value;
use tempfile::TempDir;
use time::OffsetDateTime;
use tower::ServiceExt;

/// Fixed start time for every test clock: 2026-01-01T00:00:00Z.
pub const START: i64 = 1_767_225_600;

pub const INDEX_HTML: &str = "<!doctype html><title>img-bed admin</title><div id=\"root\"></div>";
pub const APP_JS: &str = "console.log('img-bed admin');";

pub struct TestApp {
    pub router: Router,
    pub clock: ManualClock,
    pub config: Config,
    /// Owns the temporary directory; it is deleted when the TestApp drops.
    pub dir: Arc<TempDir>,
}

impl TestApp {
    /// A fresh instance with no password set.
    pub async fn new() -> Self {
        let dir = Arc::new(TempDir::new().expect("tempdir"));
        let frontend_dir = dir.path().join("web");
        std::fs::create_dir_all(frontend_dir.join("assets")).unwrap();
        std::fs::write(frontend_dir.join("index.html"), INDEX_HTML).unwrap();
        std::fs::write(frontend_dir.join("assets/app.js"), APP_JS).unwrap();
        // Written so CLI tests can run the binary against the same instance.
        std::fs::write(
            dir.path().join("img-bed.toml"),
            "data_dir = \"data\"\nfrontend_dir = \"web\"\n",
        )
        .unwrap();
        let config = Config::load(&dir.path().join("img-bed.toml")).unwrap();
        let clock = ManualClock::new(OffsetDateTime::from_unix_timestamp(START).unwrap());
        Self::build(dir, config, clock).await
    }

    /// A fresh instance whose Owner password is `password`.
    pub async fn with_password(password: &str) -> Self {
        let app = Self::new().await;
        app.set_password(password).await;
        app
    }

    /// Builds a second app on the same data directory and clock, like a
    /// server restart.
    pub async fn restart(&self) -> Self {
        Self::build(self.dir.clone(), self.config.clone(), self.clock.clone()).await
    }

    async fn build(dir: Arc<TempDir>, config: Config, clock: ManualClock) -> Self {
        let db = img_bed::db::open(&config.data_dir).await.expect("open db");
        let state = AppState::new(
            config.clone(),
            db,
            Arc::new(clock.clone()),
            Arc::new(SeededRandom::new(0)),
        );
        Self {
            router: app::router(state),
            clock,
            config,
            dir,
        }
    }

    pub fn config_path(&self) -> PathBuf {
        self.dir.path().join("img-bed.toml")
    }

    pub fn data_dir(&self) -> &Path {
        &self.config.data_dir
    }

    /// Sets the password directly (the CLI path is covered in `tests/cli.rs`).
    pub async fn set_password(&self, password: &str) {
        let db = img_bed::db::open(&self.config.data_dir).await.unwrap();
        img_bed::auth::set_password(&db, password, self.clock_now())
            .await
            .unwrap();
        db.close().await;
    }

    pub fn clock_now(&self) -> OffsetDateTime {
        img_bed::clock::Clock::now(&self.clock)
    }

    // -- HTTP -------------------------------------------------------------

    pub async fn send(&self, request: Request<Body>) -> TestResponse {
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("router is infallible");
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        TestResponse {
            status,
            headers,
            body,
        }
    }

    pub async fn request(
        &self,
        method: Method,
        uri: &str,
        session: Option<&Session>,
        json: Option<Value>,
    ) -> TestResponse {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(session) = session {
            builder = builder.header(header::COOKIE, session.cookie_header());
        }
        let body = match json {
            Some(json) => {
                builder = builder.header(header::CONTENT_TYPE, "application/json");
                Body::from(json.to_string())
            }
            None => Body::empty(),
        };
        self.send(builder.body(body).unwrap()).await
    }

    pub async fn get(&self, uri: &str) -> TestResponse {
        self.request(Method::GET, uri, None, None).await
    }

    pub async fn get_as(&self, uri: &str, session: &Session) -> TestResponse {
        self.request(Method::GET, uri, Some(session), None).await
    }

    pub async fn post_json(&self, uri: &str, json: Value) -> TestResponse {
        self.request(Method::POST, uri, None, Some(json)).await
    }

    pub async fn delete_as(&self, uri: &str, session: &Session) -> TestResponse {
        self.request(Method::DELETE, uri, Some(session), None).await
    }

    // -- Auth -------------------------------------------------------------

    pub async fn login(&self, password: &str, remember_me: bool) -> TestResponse {
        self.post_json(
            "/api/v1/session",
            serde_json::json!({ "password": password, "remember_me": remember_me }),
        )
        .await
    }

    /// Logs in and returns the session, asserting success.
    pub async fn login_ok(&self, password: &str, remember_me: bool) -> Session {
        let res = self.login(password, remember_me).await;
        assert_eq!(res.status, StatusCode::OK, "login failed: {}", res.text());
        res.session().expect("login response sets a session cookie")
    }

    /// Whether `session` is currently accepted by the server.
    pub async fn is_valid(&self, session: &Session) -> bool {
        let res = self.get_as("/api/v1/session", session).await;
        match res.status {
            StatusCode::OK => true,
            StatusCode::UNAUTHORIZED => false,
            other => panic!("unexpected status {other} from session check"),
        }
    }

    // -- CLI --------------------------------------------------------------

    /// Runs the real `img-bed` binary against this instance's config file.
    pub fn cli(&self, args: &[&str], stdin: &str) -> Output {
        run_cli(&self.config_path(), args, stdin)
    }
}

pub fn run_cli(config: &Path, args: &[&str], stdin: &str) -> Output {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_img-bed"))
        .arg("--config")
        .arg(config)
        .args(args)
        .env_remove("IMG_BED_CONFIG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn img-bed");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

/// A logged-in Owner session, as the browser would hold it.
#[derive(Debug, Clone)]
pub struct Session {
    pub token: String,
}

impl Session {
    pub fn cookie_header(&self) -> String {
        format!("{SESSION_COOKIE}={}", self.token)
    }
}

pub struct TestResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl TestResponse {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|e| panic!("body is not JSON ({e}): {}", self.text()))
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(|v| v.to_str().unwrap())
    }

    /// The `error.code` of a JSON error response.
    pub fn error_code(&self) -> String {
        self.json()["error"]["code"]
            .as_str()
            .unwrap_or_else(|| panic!("not an error response: {}", self.text()))
            .to_owned()
    }

    /// The raw `Set-Cookie` header for the session cookie, if any.
    pub fn session_set_cookie(&self) -> Option<&str> {
        self.headers
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|v| v.to_str().unwrap())
            .find(|v| v.starts_with(&format!("{SESSION_COOKIE}=")))
    }

    /// The session the response's `Set-Cookie` hands to the browser.
    pub fn session(&self) -> Option<Session> {
        let cookie = self.session_set_cookie()?;
        let value = cookie.split(';').next()?.split_once('=')?.1;
        (!value.is_empty()).then(|| Session {
            token: value.to_owned(),
        })
    }

    /// The attributes of the session `Set-Cookie`, lower-cased, e.g.
    /// `["path=/", "max-age=86400", "httponly", ...]`.
    pub fn session_cookie_attributes(&self) -> Vec<String> {
        self.session_set_cookie()
            .map(|c| {
                c.split(';')
                    .skip(1)
                    .map(|a| a.trim().to_ascii_lowercase())
                    .collect()
            })
            .unwrap_or_default()
    }
}
