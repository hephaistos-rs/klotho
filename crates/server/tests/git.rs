//! End-to-end git tests: the real `git` client against a running server.
//!
//! These are the Phase 1 "Done when" checks for clone and push. Git runs with an
//! empty config so the developer's own settings (credential helpers, URL
//! rewrites) can't change the outcome.

mod common;

use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Runs `git` in `dir` and returns stdout, failing the test with git's stderr.
fn git(dir: &Path, args: &[&str]) -> String {
    let empty_config = dir.parent().unwrap().join("empty-gitconfig");
    std::fs::write(&empty_config, "").unwrap();
    let output = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", &empty_config)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?} failed:\n{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Runs blocking git work without stalling the server on the same runtime.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(f).await.unwrap()
}

/// Makes a local repository in `work/<name>` with one commit containing `file`.
fn local_repo_with_commit(work: &Path, name: &str, file: &str) -> std::path::PathBuf {
    let dir = work.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    std::fs::write(dir.join(file), "hello\n").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "first"]);
    dir
}

#[tokio::test(flavor = "multi_thread")]
async fn push_then_clone_with_any_casing_and_optional_git_suffix() {
    let running = common::start(Duration::from_secs(5)).await;
    running.core.create_repo("alice", "MyRepo").await.unwrap();
    let work = tempfile::tempdir().unwrap();
    let (push_url, clone_url) = (running.url("/alice/myrepo.git"), running.url("/ALICE/MyRepo"));
    let work_path = work.path().to_owned();

    blocking(move || {
        let local = local_repo_with_commit(&work_path, "local", "README.md");
        git(&local, &["push", "-q", &push_url, "HEAD:refs/heads/main"]);
        git(&work_path, &["clone", "-q", &clone_url, "copy"]);
        assert!(work_path.join("copy").join("README.md").is_file());
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn windows_device_names_clone_fine() {
    let running = common::start(Duration::from_secs(5)).await;
    let work = tempfile::tempdir().unwrap();
    let work_path = work.path().to_owned();
    for name in ["con", "nul", "aux"] {
        running.core.create_repo("alice", name).await.unwrap();
        let url = running.url(&format!("/alice/{name}.git"));
        let work_path = work_path.clone();
        let name = name.to_owned();
        blocking(move || {
            let local = local_repo_with_commit(&work_path, &format!("src-{name}"), "file.txt");
            git(&local, &["push", "-q", &url, "HEAD:refs/heads/main"]);
            git(&work_path, &["clone", "-q", &url, &format!("copy-{name}")]);
        })
        .await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_adopted_legacy_repository_can_be_cloned() {
    let running = common::start(Duration::from_secs(5)).await;
    let root = running.core.store().root().to_owned();
    let work = tempfile::tempdir().unwrap();
    let work_path = work.path().to_owned();

    // A repository from before ID-based storage: `<root>/legacy.git`.
    let legacy = root.join("legacy.git");
    blocking(move || {
        git(&root, &["init", "-q", "--bare", "legacy.git"]);
        let local = local_repo_with_commit(&work_path, "local", "old.txt");
        git(&local, &["push", "-q", legacy.to_str().unwrap(), "HEAD:refs/heads/main"]);
    })
    .await;

    let repo = running.core.adopt("legacy.git", "alice", Some("Legacy")).await.unwrap();
    assert_eq!(repo.full_name(), "alice/Legacy");

    let url = running.url("/alice/legacy.git");
    let work_path = work.path().to_owned();
    blocking(move || {
        git(&work_path, &["clone", "-q", &url, "copy"]);
        assert!(work_path.join("copy").join("old.txt").is_file());
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_repositories_are_404_for_git() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let running = common::start(Duration::from_secs(5)).await;
    // A raw request: the git client would only print its own error.
    let mut conn = tokio::net::TcpStream::connect(running.addr).await.unwrap();
    let request = "GET /alice/nope.git/info/refs?service=git-upload-pack HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
    conn.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    conn.read_to_string(&mut response).await.unwrap();
    assert!(response.starts_with("HTTP/1.1 404"), "{response}");
}
