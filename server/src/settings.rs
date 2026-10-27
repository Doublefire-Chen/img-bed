//! The Owner's settings, stored in the `settings` table.
//!
//! The Path Template is read from the database once at startup and kept in
//! [`AppState`] behind a lock; [`set_path_template`] writes the database and
//! swaps the in-memory copy, so the very next upload (session or API Token)
//! uses it without a restart.

use std::sync::Arc;

use sqlx::SqlitePool;
use time::OffsetDateTime;

use crate::path_template::Template;
use crate::state::AppState;

const PATH_TEMPLATE: &str = "path_template";

/// The stored Path Template, or the default when none is stored. A stored
/// template the current rules reject (possible only after an upgrade
/// tightened them) falls back to the default with a warning.
pub async fn load_path_template(db: &SqlitePool) -> anyhow::Result<Template> {
    let stored = sqlx::query_scalar!("SELECT value FROM settings WHERE key = ?", PATH_TEMPLATE)
        .fetch_optional(db)
        .await?;
    Ok(match stored {
        None => Template::default(),
        Some(source) => Template::parse(&source).unwrap_or_else(|e| {
            tracing::warn!("stored Path Template {source:?} is invalid ({e}); using the default");
            Template::default()
        }),
    })
}

/// Writes the Path Template to the database only. Running servers pick it up
/// on restart; use [`set_path_template`] to change a running one.
pub async fn save_path_template(
    db: &SqlitePool,
    template: &Template,
    now: OffsetDateTime,
) -> anyhow::Result<()> {
    let source = template.as_str();
    let now = now.unix_timestamp();
    sqlx::query!(
        "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        PATH_TEMPLATE,
        source,
        now,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Saves the Path Template and makes it the one new uploads use.
pub async fn set_path_template(state: &AppState, template: Template) -> anyhow::Result<()> {
    // Holding the write lock across the database write keeps the stored and
    // in-memory templates the same when two saves race.
    let mut current = state.path_template.write().await;
    save_path_template(&state.db, &template, state.now()).await?;
    *current = Arc::new(template);
    Ok(())
}

/// The Path Template new uploads use. Every upload route reads it here.
pub async fn path_template(state: &AppState) -> Arc<Template> {
    state.path_template.read().await.clone()
}
