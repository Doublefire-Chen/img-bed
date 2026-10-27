//! Shared application state handed to every handler.

use std::sync::Arc;

use sqlx::SqlitePool;
use time::OffsetDateTime;

use crate::auth::LoginLimiter;
use crate::clock::SharedClock;
use crate::config::Config;
use crate::path_template::Template;
use crate::random::SharedRandom;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: SqlitePool,
    pub clock: SharedClock,
    pub random: SharedRandom,
    pub login_limiter: Arc<LoginLimiter>,
    /// The Path Template for new uploads. Fixed to the default until the
    /// settings slice stores it in the database; tests may swap it.
    pub path_template: Arc<Template>,
}

impl AppState {
    pub fn new(config: Config, db: SqlitePool, clock: SharedClock, random: SharedRandom) -> Self {
        Self {
            config: Arc::new(config),
            db,
            clock,
            random,
            login_limiter: Arc::new(LoginLimiter::default()),
            path_template: Arc::new(Template::default()),
        }
    }

    pub fn now(&self) -> OffsetDateTime {
        self.clock.now()
    }
}
