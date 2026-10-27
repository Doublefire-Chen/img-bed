//! Shared application state handed to every handler.

use std::sync::Arc;

use sqlx::SqlitePool;
use time::OffsetDateTime;
use tokio::sync::{Mutex, RwLock};

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
    /// The Path Template for new uploads, loaded from the database at
    /// startup. Read it with [`crate::settings::path_template`] and change
    /// it with [`crate::settings::set_path_template`].
    pub path_template: Arc<RwLock<Arc<Template>>>,
    /// Held while a file is created for an Image or erased by a Permanent
    /// Delete, so an upload of the same bytes can never find the old record
    /// gone but the file still there (and then lose the file), and a
    /// Duplicate Upload can't Restore an Image that is being erased.
    pub files_lock: Arc<Mutex<()>>,
}

impl AppState {
    /// Builds the state, loading the stored settings from `db`.
    pub async fn load(
        config: Config,
        db: SqlitePool,
        clock: SharedClock,
        random: SharedRandom,
    ) -> anyhow::Result<Self> {
        let path_template = crate::settings::load_path_template(&db).await?;
        Ok(Self {
            config: Arc::new(config),
            db,
            clock,
            random,
            login_limiter: Arc::new(LoginLimiter::default()),
            path_template: Arc::new(RwLock::new(Arc::new(path_template))),
            files_lock: Arc::new(Mutex::new(())),
        })
    }

    pub fn now(&self) -> OffsetDateTime {
        self.clock.now()
    }
}
