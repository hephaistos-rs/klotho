//! Klotho's HTTP server: git smart HTTP, the JSON API, operations endpoints and,
//! for every other path, the web UI.

mod api;
mod assets;
mod error;
mod git_http;

use std::future::Future;
use std::io;
use std::time::Duration;

use axum::extract::{FromRef, Request};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use klotho_core::config::ServerConfig;
use klotho_git::RepoStore;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tower::ServiceBuilder;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::Level;

#[derive(Clone)]
pub struct AppState {
    pub store: RepoStore,
    /// Background tasks that drive git processes. Shutdown waits for them.
    pub git_tasks: TaskTracker,
}

impl AppState {
    pub fn new(store: RepoStore) -> Self {
        Self { store, git_tasks: TaskTracker::new() }
    }
}

impl FromRef<AppState> for RepoStore {
    fn from_ref(state: &AppState) -> Self {
        state.store.clone()
    }
}

impl FromRef<AppState> for TaskTracker {
    fn from_ref(state: &AppState) -> Self {
        state.git_tasks.clone()
    }
}

/// Builds the whole application. Git transport, `/api`, the `/-/` operations
/// endpoints and assets are matched first; anything else goes to the web UI
/// (FR-UI-050).
pub fn build_app(state: AppState, config: &ServerConfig) -> Router {
    // Git transport has no timeout: a clone or push takes as long as it takes.
    let timeout = TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, config.request_timeout());

    let api = api::router().layer(RequestBodyLimitLayer::new(config.api_body_limit)).layer(timeout);
    let ops = Router::new().route("/-/health", get(health)).layer(timeout);
    let web = assets::router().layer(timeout);

    Router::new().nest("/api", api).merge(ops).merge(git_http::router()).merge(web).with_state(state).layer(
        ServiceBuilder::new()
            .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
            .layer(
                TraceLayer::new_for_http()
                    // The path only: query strings will carry tokens (`/-/auth/magic?token=`).
                    .make_span_with(|req: &Request| {
                        let id = req.headers().get("x-request-id").and_then(|v| v.to_str().ok());
                        tracing::info_span!(
                            "request",
                            id = id.unwrap_or("-"),
                            method = %req.method(),
                            path = req.uri().path(),
                        )
                    })
                    .on_response(DefaultOnResponse::new().level(Level::INFO)),
            )
            .layer(PropagateRequestIdLayer::x_request_id()),
    )
}

/// Liveness (NFR-OPS-022): the process is up and serving requests.
async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// Serves `app` until `shutdown` completes, then stops accepting connections and
/// waits up to `grace` for running requests and git processes to finish
/// (NFR-OPS-030).
pub async fn serve(
    listener: TcpListener,
    app: Router,
    git_tasks: TaskTracker,
    shutdown: impl Future<Output = ()> + Send + 'static,
    grace: Duration,
) -> io::Result<()> {
    let addr = listener.local_addr()?;
    let stop = CancellationToken::new();
    let server = axum::serve(listener, app).with_graceful_shutdown(stop.clone().cancelled_owned());
    let mut server = tokio::spawn(server.into_future());
    klotho_web::notify_dev_server(addr).await;

    tokio::select! {
        result = &mut server => return result.map_err(io::Error::other)?,
        () = shutdown => {}
    }

    tracing::info!(
        grace_secs = grace.as_secs(),
        "shutting down: waiting for running requests and git processes"
    );
    stop.cancel();
    git_tasks.close();
    let drained = async {
        let result = server.await;
        git_tasks.wait().await;
        result
    };
    match tokio::time::timeout(grace, drained).await {
        Ok(result) => result.map_err(io::Error::other)?,
        Err(_) => {
            tracing::warn!(git_tasks = git_tasks.len(), "grace period over, exiting with work still running");
            Ok(())
        }
    }
}
