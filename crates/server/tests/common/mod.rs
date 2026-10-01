//! Shared setup for the server tests.

#![allow(dead_code)] // each test file uses a different part

use std::net::SocketAddr;
use std::time::Duration;

use klotho_core::config::{ServerConfig, StorageConfig};
use klotho_core::{Core, Urls};
use klotho_server::AppState;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// App state on a fresh data directory, with user `alice` and her repository `demo`.
pub async fn state() -> (tempfile::TempDir, AppState) {
    state_with_url("http://localhost:3000").await
}

pub async fn state_with_url(public_url: &str) -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let paths = StorageConfig::default().resolve(dir.path());
    let core = Core::open(&paths, Urls::new(public_url).unwrap()).await.unwrap();
    core.create_user("alice").await.unwrap();
    core.create_repo("alice", "demo").await.unwrap();
    (dir, AppState::new(core))
}

/// A server running on a free port.
pub struct Running {
    pub addr: SocketAddr,
    pub core: Core,
    /// Starts graceful shutdown.
    pub stop: oneshot::Sender<()>,
    /// Ends when the server has stopped.
    pub server: JoinHandle<std::io::Result<()>>,
    pub dir: tempfile::TempDir,
}

impl Running {
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }
}

pub async fn start(grace: Duration) -> Running {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (dir, state) = state_with_url(&format!("http://{addr}")).await;
    let core = state.core.clone();
    let git_tasks = state.git_tasks.clone();
    let app = klotho_server::build_app(state, &ServerConfig::default());
    let (stop, stopped) = oneshot::channel();
    let shutdown = async {
        let _ = stopped.await;
    };
    let server = tokio::spawn(klotho_server::serve(listener, app, git_tasks, shutdown, grace));
    Running { addr, core, stop, server, dir }
}
