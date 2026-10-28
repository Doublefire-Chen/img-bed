//! Command line: `img-bed [--config FILE] serve | set-password [--username NAME]`.

use std::io::{BufRead, IsTerminal};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use clap::{Parser, Subcommand};

use crate::clock::{Clock, SystemClock};
use crate::config::Config;
use crate::random::OsRandom;
use crate::state::AppState;
use crate::{app, auth, db, trash};

#[derive(Debug, Parser)]
#[command(
    name = "img-bed",
    version,
    about = "A self-hosted, single-Owner image host"
)]
pub struct Cli {
    /// Path to the TOML config file.
    #[arg(
        short,
        long,
        global = true,
        env = "IMG_BED_CONFIG",
        default_value = "img-bed.toml"
    )]
    pub config: PathBuf,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the HTTP server.
    Serve,
    /// Set or change the Owner's username and password. Ends every existing
    /// session.
    ///
    /// On a terminal, prompts for the username (Enter keeps the current one)
    /// unless --username is given, then for the password twice. Otherwise
    /// reads the password from the first line of standard input, and
    /// --username is required the first time.
    SetPassword {
        /// The Owner's username for logging in. Without it, the current
        /// username is kept.
        #[arg(long, value_name = "NAME")]
        username: Option<String>,
    },
}

pub async fn run(cli: Cli) -> anyhow::Result<()> {
    let config = Config::load(&cli.config)?;
    match cli.command {
        Command::Serve => serve(config).await,
        Command::SetPassword { username } => set_password(config, username).await,
    }
}

async fn serve(config: Config) -> anyhow::Result<()> {
    let db = db::open(&config.data_dir).await?;
    if !config.frontend_dir.join("index.html").is_file() {
        tracing::warn!(
            "no index.html in frontend_dir {}; the admin area will 404",
            config.frontend_dir.display()
        );
    }
    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .with_context(|| format!("binding {}", config.listen))?;
    let addr = listener.local_addr()?;
    tracing::info!("links to uploaded Images start with {}/", config.public_url);
    let state = AppState::load(config, db, Arc::new(SystemClock), Arc::new(OsRandom)).await?;
    // Permanently Deletes Images trashed over 30 days ago, now and hourly.
    tokio::spawn(trash::purge_periodically(state.clone()));
    let router = app::router(state);
    // Printed (not just logged) so scripts and tests can find the port.
    println!("img-bed {} listening on http://{addr}", crate::VERSION);
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

async fn set_password(config: Config, username: Option<String>) -> anyhow::Result<()> {
    let db = db::open(&config.data_dir).await?;
    let result = set_password_in(&db, username).await;
    db.close().await;
    let username = result?;
    println!("Password set for {username:?}. All existing sessions have been ended.");
    Ok(())
}

/// Asks for whatever is missing, stores it, and returns the username in use.
async fn set_password_in(
    db: &sqlx::SqlitePool,
    username: Option<String>,
) -> anyhow::Result<String> {
    let current = auth::current_username(db).await?;
    let username = match username {
        Some(name) => Some(auth::clean_username(&name)?),
        None if std::io::stdin().is_terminal() => {
            let current = current.clone();
            tokio::task::spawn_blocking(move || prompt_username(current.as_deref())).await??
        }
        None => {
            anyhow::ensure!(
                current.is_some(),
                "no username is set yet; run again with --username <name>"
            );
            None
        }
    };
    let password = tokio::task::spawn_blocking(read_new_password).await??;
    auth::set_password(db, &password, username.as_deref(), SystemClock.now()).await?;
    Ok(username.or(current).expect("checked above"))
}

/// Prompts for the username on the terminal. Returns `None` (keep the
/// current one) when Enter is pressed and there is a current one.
fn prompt_username(current: Option<&str>) -> anyhow::Result<Option<String>> {
    use std::io::Write;
    loop {
        match current {
            Some(name) => eprint!("Username [{name}]: "),
            None => eprint!("Username: "),
        }
        std::io::stderr().flush()?;
        let mut line = String::new();
        anyhow::ensure!(
            std::io::stdin().lock().read_line(&mut line)? > 0,
            "no username given"
        );
        if line.trim().is_empty() && current.is_some() {
            return Ok(None);
        }
        match auth::clean_username(&line) {
            Ok(name) => return Ok(Some(name)),
            Err(e) => eprintln!("{e}"),
        }
    }
}

fn read_new_password() -> anyhow::Result<String> {
    if std::io::stdin().is_terminal() {
        let first = rpassword::prompt_password("New password: ")?;
        let second = rpassword::prompt_password("Repeat password: ")?;
        anyhow::ensure!(first == second, "the passwords do not match");
        Ok(first)
    } else {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        Ok(line.trim_end_matches(['\r', '\n']).to_owned())
    }
}
