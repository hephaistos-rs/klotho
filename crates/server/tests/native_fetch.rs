//! Clone and fetch served by Klotho's own protocol v2 engine (ADR 0004), checked
//! with the real `git` client. Pushes still go through the `git` program here.

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::{blocking, git, git_output};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// The engine sends this progress line on every pack, so seeing it in the
/// client's output proves the native path served the request.
const NATIVE_MARKER: &str = "Klotho: sent";

/// A history with what a fetch has to get right: branches, a merge, a file that
/// is deleted and re-added, nested directories, content similar enough between
/// versions to produce deltas, and annotated and lightweight tags.
fn rich_history(work: &Path) -> PathBuf {
    let dir = work.join("source");
    std::fs::create_dir_all(dir.join("src/nested")).unwrap();
    git(&dir, &["init", "-q"]);
    let big: String = (0..2000).map(|i| format!("line {i} of a file large enough to delta well\n")).collect();
    for i in 0..15 {
        std::fs::write(dir.join("src/nested/big.txt"), format!("{big}change {i}\n")).unwrap();
        std::fs::write(dir.join(format!("file{}.txt", i % 4)), format!("version {i}\n")).unwrap();
        if i == 5 {
            std::fs::remove_file(dir.join("file1.txt")).unwrap();
        }
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-q", "-m", &format!("commit {i}")]);
        if i == 7 {
            git(&dir, &["tag", "-a", "v1.0", "-m", "release 1.0"]);
            git(&dir, &["tag", "light"]);
        }
    }
    git(&dir, &["checkout", "-q", "-b", "feature", "HEAD~5"]);
    for i in 0..3 {
        std::fs::write(dir.join("feature.txt"), format!("feature {i}\n")).unwrap();
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-q", "-m", &format!("feature {i}")]);
    }
    git(&dir, &["checkout", "-q", "main"]);
    git(&dir, &["merge", "-q", "--no-edit", "feature"]);
    dir
}

/// Every ref with what it points at, as `git` sees it in `dir`.
fn refs(dir: &Path, pattern: &str) -> String {
    git(dir, &["for-each-ref", "--format=%(refname:lstrip=2) %(objectname)", pattern])
}

/// Pushes every branch and tag of `source` to `url` (still through the `git` program).
fn push_all(source: &Path, url: &str) {
    git(source, &["push", "-q", url, "refs/heads/*:refs/heads/*", "refs/tags/*:refs/tags/*"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_v2_advertisement_comes_from_klotho() {
    let running = common::start(Duration::from_secs(5)).await;
    let mut conn = tokio::net::TcpStream::connect(running.addr).await.unwrap();
    let request = "GET /alice/demo.git/info/refs?service=git-upload-pack HTTP/1.1\r\nHost: x\r\nGit-Protocol: version=2\r\nConnection: close\r\n\r\n";
    conn.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    conn.read_to_string(&mut response).await.unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("000eversion 2\n"), "{response}");
    assert!(response.contains("agent=klotho/"), "{response}");
}

#[tokio::test(flavor = "multi_thread")]
async fn clone_fetch_and_tags_through_the_native_engine() {
    let running = common::start(Duration::from_secs(5)).await;
    let url = running.url("/alice/demo.git");
    let server_repo = running.core.store().path(running.core.find_repo("alice", "demo").await.unwrap().id);
    let work = tempfile::tempdir().unwrap();
    let work = work.path().to_owned();

    blocking(move || {
        let source = rich_history(&work);
        push_all(&source, &url);

        // A fresh clone, from loose objects (small pushes are unpacked).
        let (_, stderr) = git_output(&work, &["clone", "--progress", &url, "clone1"]);
        assert!(stderr.contains(NATIVE_MARKER), "not served natively:\n{stderr}");
        let clone1 = work.join("clone1");
        git(&clone1, &["fsck", "--full", "--strict"]);
        assert_eq!(refs(&clone1, "refs/remotes/origin/"), refs_as_remote(&source));
        assert_eq!(refs(&clone1, "refs/tags/"), refs(&source, "refs/tags/"));
        assert_eq!(git(&clone1, &["cat-file", "-t", "v1.0"]).trim(), "tag");

        // The same after the server repository is fully packed, so the pack is
        // built by copying entries, deltas included.
        git(&server_repo, &["repack", "-a", "-d", "-f", "-q"]);
        let (_, stderr) = git_output(&work, &["clone", "--progress", &url, "clone2"]);
        assert!(stderr.contains(NATIVE_MARKER), "{stderr}");
        git(&work.join("clone2"), &["fsck", "--full", "--strict"]);

        // An incremental fetch: the client sends haves, the engine acknowledges
        // them and sends only what's new, as a thin pack.
        for i in 0..3 {
            std::fs::write(source.join("src/nested/big.txt"), format!("rewritten {i}\n")).unwrap();
            git(&source, &["commit", "-q", "-a", "-m", &format!("later {i}")]);
        }
        git(&source, &["tag", "-a", "v2.0", "-m", "release 2.0"]);
        push_all(&source, &url);
        let (_, stderr) = git_output(&clone1, &["fetch", "--progress", "--tags", "origin"]);
        assert!(stderr.contains(NATIVE_MARKER), "{stderr}");
        git(&clone1, &["fsck", "--full", "--strict"]);
        assert_eq!(refs(&clone1, "refs/remotes/origin/"), refs_as_remote(&source));
        assert_eq!(git(&clone1, &["cat-file", "-t", "v2.0"]).trim(), "tag");
    })
    .await;
}

/// `source`'s branches as a clone of it names them (`origin/main`), plus `origin/HEAD`.
fn refs_as_remote(source: &Path) -> String {
    let branches = refs(source, "refs/heads/");
    let head = git(source, &["rev-parse", "HEAD"]);
    let mut lines: Vec<String> = branches.lines().map(|line| format!("origin/{line}")).collect();
    lines.push(format!("origin/HEAD {}", head.trim()));
    lines.sort();
    lines.iter().map(|line| format!("{line}\n")).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_empty_repository_clones() {
    let running = common::start(Duration::from_secs(5)).await;
    let url = running.url("/alice/demo.git");
    let work = tempfile::tempdir().unwrap();
    let work = work.path().to_owned();
    blocking(move || {
        let (_, stderr) = git_output(&work, &["clone", &url, "empty"]);
        assert!(stderr.contains("empty repository"), "{stderr}");
        // The unborn HEAD's branch comes from the server (`ls-refs unborn`).
        assert_eq!(git(&work.join("empty"), &["symbolic-ref", "HEAD"]).trim(), "refs/heads/main");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn protocol_v0_still_works_through_the_fallback() {
    let running = common::start(Duration::from_secs(5)).await;
    let url = running.url("/alice/demo.git");
    let work = tempfile::tempdir().unwrap();
    let work = work.path().to_owned();
    blocking(move || {
        let source = rich_history(&work);
        push_all(&source, &url);
        let (_, stderr) =
            git_output(&work, &["-c", "protocol.version=0", "clone", "--progress", &url, "old"]);
        assert!(!stderr.contains(NATIVE_MARKER), "{stderr}");
        git(&work.join("old"), &["fsck", "--full"]);
    })
    .await;
}

/// Sends one v2 request body to `git-upload-pack` and returns the response body.
async fn v2_request(addr: std::net::SocketAddr, body: &[u8]) -> Vec<u8> {
    let mut conn = tokio::net::TcpStream::connect(addr).await.unwrap();
    let head = format!(
        "POST /alice/demo.git/git-upload-pack HTTP/1.1\r\nHost: x\r\nGit-Protocol: version=2\r\n\
         Content-Type: application/x-git-upload-pack-request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    conn.write_all(head.as_bytes()).await.unwrap();
    conn.write_all(body).await.unwrap();
    let mut response = Vec::new();
    conn.read_to_end(&mut response).await.unwrap();
    let split = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    // The body is chunked; for these short answers the pkt-lines are readable
    // between the chunk sizes, which is all the assertions need.
    response[split..].to_vec()
}

fn pkt(line: &str) -> String {
    format!("{:04x}{line}\n", line.len() + 5)
}

#[tokio::test(flavor = "multi_thread")]
async fn objects_that_no_ref_points_at_are_refused() {
    let running = common::start(Duration::from_secs(5)).await;
    let url = running.url("/alice/demo.git");
    let work = tempfile::tempdir().unwrap();
    let work_path = work.path().to_owned();
    let blob = blocking(move || {
        let source = rich_history(&work_path);
        push_all(&source, &url);
        // A blob that exists in the repository but isn't a ref tip.
        git(&source, &["rev-parse", "HEAD:file0.txt"]).trim().to_owned()
    })
    .await;

    let body = format!("{}0001{}{}0000", pkt("command=fetch"), pkt(&format!("want {blob}")), pkt("done"));
    let response = String::from_utf8_lossy(&v2_request(running.addr, body.as_bytes()).await).into_owned();
    assert!(response.contains(&format!("ERR upload-pack: not our ref {blob}")), "{response}");
    assert!(!response.contains("packfile"), "{response}");
}

#[tokio::test(flavor = "multi_thread")]
async fn ls_refs_honours_prefixes_and_peels_tags() {
    let running = common::start(Duration::from_secs(5)).await;
    let url = running.url("/alice/demo.git");
    let work = tempfile::tempdir().unwrap();
    let work_path = work.path().to_owned();
    let (tag, commit) = blocking(move || {
        let source = rich_history(&work_path);
        push_all(&source, &url);
        (
            git(&source, &["rev-parse", "v1.0"]).trim().to_owned(),
            git(&source, &["rev-parse", "v1.0^{}"]).trim().to_owned(),
        )
    })
    .await;

    let body = format!("{}0001{}{}0000", pkt("command=ls-refs"), pkt("peel"), pkt("ref-prefix refs/tags/v"));
    let response = String::from_utf8_lossy(&v2_request(running.addr, body.as_bytes()).await).into_owned();
    assert!(response.contains(&format!("{tag} refs/tags/v1.0 peeled:{commit}")), "{response}");
    assert!(!response.contains("refs/heads/"), "{response}");
    assert!(!response.contains("refs/tags/light"), "{response}");
}

#[tokio::test(flavor = "multi_thread")]
async fn shallow_clones_keep_working() {
    let running = common::start(Duration::from_secs(5)).await;
    let url = running.url("/alice/demo.git");
    let work = tempfile::tempdir().unwrap();
    let work = work.path().to_owned();
    blocking(move || {
        let source = rich_history(&work);
        push_all(&source, &url);
        git(&work, &["clone", "-q", "--depth", "1", &url, "shallow"]);
        let shallow = work.join("shallow");
        assert_eq!(git(&shallow, &["rev-list", "--count", "HEAD"]).trim(), "1");
        git(&shallow, &["fetch", "-q", "--deepen", "2"]);
        let deeper: u32 = git(&shallow, &["rev-list", "--count", "HEAD"]).trim().parse().unwrap();
        assert!(deeper > 1, "--deepen fetched nothing more");
        git(&shallow, &["fsck"]);
    })
    .await;
}
