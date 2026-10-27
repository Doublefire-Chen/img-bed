//! The SQLite database in the data directory, and its Migrations.

use std::path::Path;
use std::str::FromStr;

use anyhow::Context;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

pub const DB_FILE: &str = "img-bed.db";

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Creates the data directory if needed, opens (or creates) the database in
/// it and applies any pending Migrations. Safe to call on every start.
pub async fn open(data_dir: &Path) -> anyhow::Result<SqlitePool> {
    std::fs::create_dir_all(data_dir)
        .with_context(|| format!("creating data directory {}", data_dir.display()))?;
    let path = data_dir.join(DB_FILE);
    let options = SqliteConnectOptions::from_str("sqlite:")?
        .filename(&path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await
        .with_context(|| format!("opening database {}", path.display()))?;
    MIGRATOR
        .run(&pool)
        .await
        .context("applying database migrations")?;
    Ok(pool)
}
