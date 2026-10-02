//! Shared setup for the server tests.

#![allow(dead_code)] // each test file uses a different part

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
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

/// Runs `git` in `dir` and returns (stdout, stderr), failing the test if git fails.
///
/// Git runs with an empty config so the developer's own settings (credential
/// helpers, URL rewrites, protocol versions) can't change the outcome.
pub fn git_output(dir: &Path, args: &[&str]) -> (String, String) {
    git_output_env(dir, &[], args)
}

/// [`git_output`] with extra environment variables, e.g. `GIT_COMMITTER_DATE`.
pub fn git_output_env(dir: &Path, env: &[(&str, &str)], args: &[&str]) -> (String, String) {
    let output = git_command(dir, args).envs(env.iter().copied()).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(output.status.success(), "git {args:?} failed:\n{stderr}");
    (String::from_utf8_lossy(&output.stdout).into_owned(), stderr)
}

/// Runs `git` in `dir` and returns whether it succeeded, with its stderr.
pub fn git_try(dir: &Path, args: &[&str]) -> (bool, String) {
    let output = git_command(dir, args).output().unwrap();
    (output.status.success(), String::from_utf8_lossy(&output.stderr).into_owned())
}

/// `git` with `args`, an empty config and a test identity.
pub fn git_command(dir: &Path, args: &[&str]) -> Command {
    let empty_config = std::env::temp_dir().join(format!("klotho-test-gitconfig-{}", std::process::id()));
    if !empty_config.exists() {
        std::fs::write(&empty_config, "").unwrap();
    }
    let mut command = Command::new("git");
    command
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", &empty_config)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0");
    command
}

/// Runs `git` in `dir` and returns stdout, failing the test with git's stderr.
pub fn git(dir: &Path, args: &[&str]) -> String {
    git_output(dir, args).0
}

/// Runs blocking git work without stalling the server on the same runtime.
pub async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(f).await.unwrap()
}

/// Makes a local repository in `work/<name>` with one commit containing `file`.
pub fn local_repo_with_commit(work: &Path, name: &str, file: &str) -> PathBuf {
    let dir = work.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    std::fs::write(dir.join(file), "hello\n").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "first"]);
    dir
}
