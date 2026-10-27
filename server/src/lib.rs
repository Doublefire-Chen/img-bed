//! img-bed: a self-hosted, single-Owner image host.
//!
//! The binary (`main.rs`) is a thin wrapper around [`cli::run`]. Everything
//! else lives here so integration tests can build the whole app in-process
//! (see `tests/common`).

pub mod api;
pub mod app;
pub mod auth;
pub mod cli;
pub mod clock;
pub mod config;
pub mod db;
pub mod error;
pub mod frontend;
pub mod image_type;
pub mod path;
pub mod path_template;
pub mod random;
pub mod serve;
pub mod settings;
pub mod state;
pub mod store;
pub mod trash;

/// The server version, reported by `GET /api/v1/info`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
