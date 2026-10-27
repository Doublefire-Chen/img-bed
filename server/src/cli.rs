//! Command line: `img-bed [--config FILE] serve | set-password`.

use std::io::{BufRead, IsTerminal};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use clap::{Parser, Subcommand};

use crate::clock::{Clock, SystemClock};
use crate::config::Config;
use crate::random::OsRandom;
use crate::state::AppState;
use crate::{app, auth, db};

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
    /// Set or change the Owner's password. Ends every existing session.
    ///
    /// Prompts for the password on a terminal; otherwise reads the first
    /// line of standard input.
    SetPassword,
}

pub async fn run(cli: Cli) -> anyhow::Result<()> {
    let config = Config::load(&cli.config)?;
    match cli.command {
        Command::Serve => serve(config).await,
        Command::SetPassword => set_password(config).await,
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
    let state = AppState::new(config, db, Arc::new(SystemClock), Arc::new(OsRandom));
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

async fn set_password(config: Config) -> anyhow::Result<()> {
    let password = tokio::task::spawn_blocking(read_new_password).await??;
    let db = db::open(&config.data_dir).await?;
    auth::set_password(&db, &password, SystemClock.now()).await?;
    db.close().await;
    println!("Password set. All existing sessions have been ended.");
    Ok(())
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
