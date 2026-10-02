mod config;

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use klotho_core::{Core, NewUser, Urls};
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
    /// Administration tasks that work without the web UI or the server running
    /// (NFR-OPS-033).
    #[command(subcommand)]
    Admin(Admin),
}

#[derive(Subcommand)]
enum Admin {
    /// Create an account. Asks for its password unless `--password-stdin` is given.
    /// The first administrator is made this way (FR-AUTH-003).
    CreateUser {
        username: String,
        #[arg(long)]
        email: Option<String>,
        /// Make it an instance administrator.
        #[arg(long)]
        admin: bool,
        /// Read the password from the first line of stdin, for scripts.
        #[arg(long)]
        password_stdin: bool,
    },
    /// Print a single-use sign-up link, for `auth.registration = "invite"`.
    Invite,
    /// List repositories on disk that aren't registered, and registered ones whose
    /// directory is missing.
    Unadopted,
    /// Register a repository from `admin unadopted` under an owner.
    Adopt {
        /// The path `admin unadopted` printed, e.g. `demo.git`.
        path: String,
        #[arg(long)]
        owner: String,
        /// The repository name. Defaults to the directory name without `.git`.
        #[arg(long)]
        name: Option<String>,
    },
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
        Command::Admin(admin) => run_admin(open_core(&loaded).await?, admin).await,
    }
}

/// Opens the database (migrating it) and the repository store.
async fn open_core(loaded: &config::Loaded) -> anyhow::Result<Core> {
    let urls = Urls::new(&loaded.config.server.public_url).map_err(anyhow::Error::msg)?;
    let paths = loaded.config.storage.resolve(&loaded.base_dir);
    tracing::info!(
        data_dir = %paths.data_dir.display(),
        repositories = %paths.repositories.display(),
        database = %paths.database.display(),
        files = %paths.files.display(),
        "data locations"
    );
    Core::open(&paths, urls, loaded.config.auth.clone())
        .await
        .with_context(|| format!("can't open the data in {}", paths.data_dir.display()))
}

async fn run_admin(core: Core, admin: Admin) -> anyhow::Result<()> {
    match admin {
        Admin::CreateUser { username, email, admin, password_stdin } => {
            let password = if password_stdin {
                let mut line = String::new();
                std::io::stdin().read_line(&mut line).context("can't read the password from stdin")?;
                line.trim_end_matches(['\r', '\n']).to_owned()
            } else {
                let password = rpassword::prompt_password(format!("Password for {username}: "))?;
                if rpassword::prompt_password("Again: ")? != password {
                    anyhow::bail!("the passwords don't match");
                }
                password
            };
            let new =
                NewUser { username: &username, email: email.as_deref(), password: Some(&password), admin };
            let user = core.create_user(new).await?;
            let role = if user.is_admin { "administrator" } else { "user" };
            println!("created {role} {} (id {})", user.username, user.id);
        }
        Admin::Invite => {
            let secret = core.create_invite().await?;
            let days = klotho_core::INVITE_LIFETIME.as_secs() / (24 * 60 * 60);
            println!("{}", core.urls().absolute(&format!("/-/register?invite={secret}")));
            println!("The link works once, for {days} days, while registration is set to \"invite\".");
        }
        Admin::Unadopted => {
            let report = core.storage_report().await?;
            for path in &report.unadopted {
                println!("unadopted  {path}");
            }
            for repo in &report.missing {
                println!(
                    "missing    {} (id {}, expected at {})",
                    repo.full_name(),
                    repo.id,
                    core.store().path(repo.id).display()
                );
            }
            if report.unadopted.is_empty() && report.missing.is_empty() {
                println!("storage and database agree");
            }
        }
        Admin::Adopt { path, owner, name } => {
            let repo = core.adopt(&path, &owner, name.as_deref()).await?;
            println!("adopted {path} as {} (id {})", repo.full_name(), repo.id);
        }
    }
    Ok(())
}

async fn serve(loaded: config::Loaded) -> anyhow::Result<()> {
    match &loaded.file {
        Some(file) => tracing::info!(file = %file.display(), "loaded config"),
        None => tracing::warn!(
            base = %loaded.base_dir.display(),
            "no config file given (--config or KLOTHO_CONFIG); relative paths resolve against the current directory"
        ),
    }

    let core = open_core(&loaded).await?;
    // Disk and database disagreeing is reported, never silently ignored (FR-STOR-020).
    let report = core.storage_report().await?;
    if !report.unadopted.is_empty() || !report.missing.is_empty() {
        tracing::warn!(
            unadopted = report.unadopted.len(),
            missing = report.missing.len(),
            "repository storage and database disagree; see `klotho admin unadopted`"
        );
    }

    let config = loaded.config;
    let state = AppState::new(core);
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
