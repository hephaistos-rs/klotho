//! Pushes served by Klotho's own receive-pack (ADR 0004), checked with the real
//! `git` client and with hand-made requests for what a well-behaved client
//! never sends.

mod common;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use common::{blocking, git, git_command, git_try, local_repo_with_commit};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn commit(dir: &Path, file: &str, content: &str) -> String {
    std::fs::write(dir.join(file), content).unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", &format!("{file}: {} bytes", content.len())]);
    git(dir, &["rev-parse", "HEAD"]).trim().to_owned()
}

/// The server's refs as `git ls-remote` shows them.
fn remote_refs(dir: &Path, url: &str) -> String {
    git(dir, &["ls-remote", "--refs", url])
}

async fn server_path(running: &common::Running) -> PathBuf {
    running.core.store().path(running.core.find_repo("alice", "demo").await.unwrap().id)
}

#[tokio::test(flavor = "multi_thread")]
async fn push_force_push_delete_and_tags() {
    let running = common::start(Duration::from_secs(5)).await;
    let url = running.url("/alice/demo.git");
    let server = server_path(&running).await;
    let work = tempfile::tempdir().unwrap();
    let work = work.path().to_owned();

    blocking(move || {
        let local = local_repo_with_commit(&work, "local", "README.md");
        let big: String = (0..3000).map(|i| format!("line {i} of a file that deltas well\n")).collect();
        commit(&local, "big.txt", &big);
        git(&local, &["push", "-q", &url, "main"]);
        git(&server, &["fsck", "--full", "--strict"]);

        // A second push is a thin pack: the changed file is a delta against
        // the version the server already has.
        let main = commit(&local, "big.txt", &format!("{big}one more line\n"));
        git(&local, &["checkout", "-q", "-b", "feature"]);
        let feature = commit(&local, "feature.txt", "feature\n");
        git(&local, &["tag", "-a", "v1", "-m", "release 1", "main"]);
        git(&local, &["tag", "light", "feature"]);
        git(&local, &["push", "-q", "--tags", &url, "main", "feature"]);
        git(&server, &["fsck", "--full", "--strict"]);
        let refs = remote_refs(&work, &url);
        assert!(refs.contains(&format!("{main}\trefs/heads/main")), "{refs}");
        assert!(refs.contains(&format!("{feature}\trefs/heads/feature")), "{refs}");
        assert!(refs.contains(&format!("{feature}\trefs/tags/light")), "{refs}");
        assert!(refs.contains("\trefs/tags/v1"), "{refs}");

        // Force push: main rewritten.
        git(&local, &["checkout", "-q", "main"]);
        git(&local, &["commit", "-q", "--amend", "-m", "rewritten"]);
        let rewritten = git(&local, &["rev-parse", "HEAD"]).trim().to_owned();
        let (ok, stderr) = git_try(&local, &["push", "-q", &url, "main"]);
        assert!(!ok, "a non-fast-forward push without --force must fail:\n{stderr}");
        git(&local, &["push", "-q", "--force", &url, "main"]);
        assert!(remote_refs(&work, &url).contains(&format!("{rewritten}\trefs/heads/main")));

        // Delete a branch and a tag.
        git(&local, &["push", "-q", &url, ":feature", ":refs/tags/light"]);
        let refs = remote_refs(&work, &url);
        assert!(!refs.contains("refs/heads/feature"), "{refs}");
        assert!(!refs.contains("refs/tags/light"), "{refs}");

        // Protocol v1, and push options (accepted and, for now, unused).
        commit(&local, "v1.txt", "over v1\n");
        git(&local, &["-c", "protocol.version=1", "push", "-q", "-o", "ci.skip", &url, "main"]);
        let head = git(&local, &["rev-parse", "HEAD"]).trim().to_owned();
        assert!(remote_refs(&work, &url).contains(&format!("{head}\trefs/heads/main")));
        git(&server, &["fsck", "--full", "--strict"]);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_atomic_push_with_one_rejected_ref_changes_nothing() {
    let running = common::start(Duration::from_secs(5)).await;
    let url = running.url("/alice/demo.git");
    let work = tempfile::tempdir().unwrap();
    let work = work.path().to_owned();

    blocking(move || {
        let local = local_repo_with_commit(&work, "local", "README.md");
        git(&local, &["push", "-q", &url, "main"]);
        git(&local, &["branch", "other"]);

        // Deleting the branch HEAD points at is refused, so `other` isn't created.
        let (ok, stderr) = git_try(&local, &["push", "--atomic", &url, ":main", "other"]);
        assert!(!ok, "{stderr}");
        assert!(stderr.contains("deletion of the current branch prohibited"), "{stderr}");
        let refs = remote_refs(&work, &url);
        assert!(refs.contains("refs/heads/main"), "{refs}");
        assert!(!refs.contains("refs/heads/other"), "{refs}");

        // Without --atomic the other update goes through.
        let (ok, stderr) = git_try(&local, &["push", &url, ":main", "other"]);
        assert!(!ok, "{stderr}");
        let refs = remote_refs(&work, &url);
        assert!(refs.contains("refs/heads/main"), "{refs}");
        assert!(refs.contains("refs/heads/other"), "{refs}");
    })
    .await;
}

/// Sends a raw `git-receive-pack` request and returns the whole HTTP response.
async fn receive_pack_request(addr: std::net::SocketAddr, body: &[u8]) -> String {
    let mut conn = tokio::net::TcpStream::connect(addr).await.unwrap();
    let head = format!(
        "POST /alice/demo.git/git-receive-pack HTTP/1.1\r\nHost: x\r\n\
         Content-Type: application/x-git-receive-pack-request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    conn.write_all(head.as_bytes()).await.unwrap();
    conn.write_all(body).await.unwrap();
    let mut response = Vec::new();
    conn.read_to_end(&mut response).await.unwrap();
    String::from_utf8_lossy(&response).into_owned()
}

fn pkt(line: &str) -> Vec<u8> {
    format!("{:04x}{line}\n", line.len() + 5).into_bytes()
}

/// A pack holding exactly `ids`, without what they point at.
fn pack_of(dir: &Path, ids: &[&str]) -> Vec<u8> {
    let mut child = git_command(dir, &["pack-objects", "--stdout", "-q"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(format!("{}\n", ids.join("\n")).as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    output.stdout
}

fn pack_files(server: &Path) -> Vec<PathBuf> {
    match std::fs::read_dir(server.join("objects/pack")) {
        Ok(entries) => entries.map(|entry| entry.unwrap().path()).collect(),
        Err(_) => Vec::new(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pack_missing_objects_is_rejected_and_never_stored() {
    let running = common::start(Duration::from_secs(5)).await;
    let server = server_path(&running).await;
    let work = tempfile::tempdir().unwrap();
    let work_path = work.path().to_owned();
    let (commit, tree, pack) = blocking(move || {
        let local = local_repo_with_commit(&work_path, "local", "README.md");
        let commit = git(&local, &["rev-parse", "HEAD"]).trim().to_owned();
        let tree = git(&local, &["rev-parse", "HEAD^{tree}"]).trim().to_owned();
        // The commit, but not its tree.
        let pack = pack_of(&local, &[&commit]);
        (commit, tree, pack)
    })
    .await;
    let packs_before = pack_files(&server);

    let zero = "0".repeat(40);
    let mut body = pkt(&format!("{zero} {commit} refs/heads/broken\0report-status side-band-64k"));
    body.extend(b"0000");
    body.extend(&pack);
    let response = receive_pack_request(running.addr, &body).await;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("ng refs/heads/broken missing necessary objects"), "{response}");
    assert!(response.contains(&format!("object {tree} is missing (needed by {commit})")), "{response}");

    let repo = running.core.store().open(running.core.find_repo("alice", "demo").await.unwrap().id).unwrap();
    assert!(repo.try_find_reference("refs/heads/broken").unwrap().is_none());
    assert_eq!(pack_files(&server), packs_before, "the rejected pack was moved into the repository");
    assert_quarantine_emptied(&running).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_interrupted_push_leaves_nothing_behind() {
    let running = common::start(Duration::from_secs(5)).await;
    let server = server_path(&running).await;
    let work = tempfile::tempdir().unwrap();
    let work_path = work.path().to_owned();
    let (commit, pack) = blocking(move || {
        let local = local_repo_with_commit(&work_path, "local", "README.md");
        let big: String =
            (0..20_000).map(|i| format!("{i} some content that won't compress to nothing {i}\n")).collect();
        commit(&local, "big.txt", &big);
        let commit = git(&local, &["rev-parse", "HEAD"]).trim().to_owned();
        let objects = git(&local, &["rev-list", "--objects", "HEAD"]);
        let ids: Vec<&str> = objects.lines().map(|line| &line[..40]).collect();
        (commit, pack_of(&local, &ids))
    })
    .await;
    let packs_before = pack_files(&server);

    let zero = "0".repeat(40);
    let mut body = pkt(&format!("{zero} {commit} refs/heads/main\0report-status side-band-64k"));
    body.extend(b"0000");
    body.extend(&pack[..pack.len() / 2]);
    let mut conn = tokio::net::TcpStream::connect(running.addr).await.unwrap();
    let head = format!(
        "POST /alice/demo.git/git-receive-pack HTTP/1.1\r\nHost: x\r\n\
         Content-Type: application/x-git-receive-pack-request\r\nContent-Length: {}\r\n\r\n",
        body.len() + pack.len()
    );
    conn.write_all(head.as_bytes()).await.unwrap();
    conn.write_all(&body).await.unwrap();
    // Give the server time to start indexing, then hang up mid-pack.
    tokio::time::sleep(Duration::from_millis(300)).await;
    drop(conn);

    assert_quarantine_emptied(&running).await;
    let repo = running.core.store().open(running.core.find_repo("alice", "demo").await.unwrap().id).unwrap();
    assert!(repo.try_find_reference("refs/heads/main").unwrap().is_none());
    assert_eq!(pack_files(&server), packs_before);
}

/// Waits until no push quarantine is left in the store's scratch directory.
async fn assert_quarantine_emptied(running: &common::Running) {
    let tmp = running.core.store().tmp_dir();
    for _ in 0..100 {
        let leftovers = match std::fs::read_dir(&tmp) {
            Ok(entries) => entries.count(),
            Err(_) => 0,
        };
        if leftovers == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("a quarantine directory was left in {}", tmp.display());
}
