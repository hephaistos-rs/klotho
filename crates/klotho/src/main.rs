mod config;

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use klotho_git::RepoStore;
use klotho_server::AppState;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

/// Klotho: a self-hosted git forge.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Config file. Relative paths in it resolve against its folder.
    #[arg(long, short, env = "KLOTHO_CONFIG", global = true)]
    config: Option<PathBuf>,

    /// What to do. Defaults to `serve`.
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the server.
    Serve,
    /// Administration tasks that work without the web UI. None yet.
    Admin,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let loaded = config::load(cli.config.as_deref())?;

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&loaded.config.log.filter));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => serve(loaded).await,
        Command::Admin => anyhow::bail!("there are no admin commands yet"),
    }
}

async fn serve(loaded: config::Loaded) -> anyhow::Result<()> {
    let config = loaded.config;
    match &loaded.file {
        Some(file) => tracing::info!(file = %file.display(), "loaded config"),
        None => tracing::warn!(
            base = %loaded.base_dir.display(),
            "no config file given (--config or KLOTHO_CONFIG); relative paths resolve against the current directory"
        ),
    }

    let git = klotho_git::check_git()?;
    tracing::info!(%git, "found git");

    let paths = config.storage.resolve(&loaded.base_dir);
    tracing::info!(
        data_dir = %paths.data_dir.display(),
        repositories = %paths.repositories.display(),
        database = %paths.database.display(),
        files = %paths.files.display(),
        "data locations"
    );
    let store = RepoStore::new(&paths.repositories)
        .with_context(|| format!("can't use {} for repositories", paths.repositories.display()))?;

    let state = AppState::new(store);
    let git_tasks = state.git_tasks.clone();
    let app = klotho_server::build_app(state, &config.server);

    let addr = topcoat_dev_addr().unwrap_or(config.server.addr);
    let listener = TcpListener::bind(addr).await.with_context(|| format!("can't listen on {addr}"))?;
    tracing::info!(%addr, "Klotho listening");

    klotho_server::serve(listener, app, git_tasks, shutdown_signal(), config.server.shutdown_grace()).await?;
    tracing::info!("stopped");
    Ok(())
}

/// Under `topcoat dev`, listen on the address it picked for the app, which it
/// passes in `HOST` and `PORT`.
fn topcoat_dev_addr() -> Option<SocketAddr> {
    std::env::var_os("TOPCOAT_DEV_URL")?;
    let (host, port) = (std::env::var("HOST").ok()?, std::env::var("PORT").ok()?);
    format!("{host}:{port}").parse().ok()
}

/// Ctrl-C everywhere, plus SIGTERM on Unix (what systemd and container runtimes send).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.expect("failed to listen for Ctrl-C");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to listen for SIGTERM")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}
