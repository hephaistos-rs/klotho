mod api;
mod error;
mod smart_http;

use std::env;
use std::net::SocketAddr;

use axum::Router;
use git::RepoStore;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "klotho=info".into()))
        .init();

    // TODO: move to a config file once there is more to configure.
    let addr: SocketAddr = env::var("KLOTHO_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:3000".into())
        .parse()?;
    let store = RepoStore::new(env::var("KLOTHO_REPOS").unwrap_or_else(|_| "gitRepos".into()))?;

    tracing::info!(%addr, repos = %store.root().display(), "Klotho listening");

    let app = Router::new()
        .nest("/api", api::router())
        .merge(smart_http::router())
        .with_state(store);
    axum::serve(TcpListener::bind(addr).await?, app).await?;
    Ok(())
}
