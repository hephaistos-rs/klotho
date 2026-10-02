//! End-to-end git tests: the real `git` client against a running server.
//!
//! These are the Phase 1 "Done when" checks for clone and push. Git runs with an
//! empty config so the developer's own settings (credential helpers, URL
//! rewrites) can't change the outcome.

mod common;

use std::time::Duration;

use common::{blocking, git, local_repo_with_commit};

#[tokio::test(flavor = "multi_thread")]
async fn push_then_clone_with_any_casing_and_optional_git_suffix() {
    let running = common::start(Duration::from_secs(5)).await;
    running.core.create_repo("alice", "MyRepo", false).await.unwrap();
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
        running.core.create_repo("alice", name, false).await.unwrap();
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

/// Unknown repositories: anonymous callers get the credential challenge (the
/// repository might be private), signed-in callers a plain 404.
#[tokio::test(flavor = "multi_thread")]
async fn unknown_repositories_for_git() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let running = common::start(Duration::from_secs(5)).await;
    // Raw requests: the git client would only print its own error.
    let send = |authorization: Option<String>| async move {
        let mut conn = tokio::net::TcpStream::connect(running.addr).await.unwrap();
        let auth = authorization.map(|value| format!("Authorization: {value}\r\n")).unwrap_or_default();
        let request = format!(
            "GET /alice/nope.git/info/refs?service=git-upload-pack HTTP/1.1\r\nHost: x\r\n{auth}Connection: close\r\n\r\n"
        );
        conn.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        conn.read_to_string(&mut response).await.unwrap();
        response
    };
    let anonymous = send(None).await;
    assert!(anonymous.starts_with("HTTP/1.1 401"), "{anonymous}");
    assert!(anonymous.to_ascii_lowercase().contains("www-authenticate: basic"), "{anonymous}");

    let signed_in = send(Some(running.basic_auth())).await;
    assert!(signed_in.starts_with("HTTP/1.1 404"), "{signed_in}");
}

/// What the real git client pushes reads back through the `klotho-core` read
/// services: branches, an annotated tag, the log with its original UTC offsets
/// (FR-API-011), and `ref/path` specs split at the longest branch.
#[tokio::test(flavor = "multi_thread")]
async fn pushed_history_reads_back_through_the_core_services() {
    use common::git_output_env;
    use klotho_core::{Action, Actor, LogQuery};

    let running = common::start(Duration::from_secs(5)).await;
    let work = tempfile::tempdir().unwrap();
    let (work_path, url) = (work.path().to_owned(), running.url("/alice/demo.git"));
    blocking(move || {
        let local = work_path.join("local");
        std::fs::create_dir_all(local.join("src")).unwrap();
        git(&local, &["init", "-q"]);
        for (n, date) in [(1, "2024-01-02T03:04:05+05:30"), (2, "2024-02-03T04:05:06-08:00")] {
            std::fs::write(local.join("src").join("lib.rs"), format!("// {n}\n")).unwrap();
            git(&local, &["add", "."]);
            let env = [("GIT_AUTHOR_DATE", date), ("GIT_COMMITTER_DATE", date)];
            git_output_env(&local, &env, &["commit", "-q", "-m", &format!("change {n}")]);
        }
        git(&local, &["tag", "-a", "v1", "-m", "Version 1"]);
        git(&local, &["checkout", "-q", "-b", "feature/login"]);
        std::fs::write(local.join("other.txt"), "x\n").unwrap();
        git(&local, &["add", "."]);
        git(&local, &["commit", "-q", "-m", "other file"]);
        git(&local, &["push", "-q", &url, "main", "feature/login", "v1"]);
    })
    .await;

    let core = &running.core;
    let repo = core.repo_for(&Actor::Anonymous, "alice", "demo", Action::Read).await.unwrap();
    let branches = core.branches(&repo, None, None).await.unwrap();
    let names: Vec<_> = branches.items.iter().map(|b| (b.name.as_str(), b.default)).collect();
    assert_eq!(names, [("feature/login", false), ("main", true)]);

    let log = core.commits(&repo, LogQuery { limit: Some(1), ..LogQuery::default() }).await.unwrap();
    assert_eq!(log.items[0].summary, "change 2");
    assert_eq!(log.items[0].author.date.to_string(), "2024-02-03T04:05:06-08:00");
    let rest = LogQuery { cursor: log.next, ..LogQuery::default() };
    let rest = core.commits(&repo, rest).await.unwrap();
    assert_eq!(rest.items[0].committer.date.to_string(), "2024-01-02T03:04:05+05:30");
    assert!(rest.next.is_none());

    let only_other =
        LogQuery { rev: Some("feature/login".into()), path: "other.txt".into(), ..LogQuery::default() };
    let only_other = core.commits(&repo, only_other).await.unwrap();
    assert_eq!(only_other.items.len(), 1);

    let tag = core.tag(&repo, "v1").await.unwrap();
    assert_eq!(tag.annotation.unwrap().message.trim(), "Version 1");
    assert_eq!(tag.commit.unwrap().summary, "change 2");

    let resolved = core.resolve_spec(&repo, "feature/login/src/lib.rs").await.unwrap();
    assert_eq!((resolved.target.name.as_str(), resolved.path.as_str()), ("feature/login", "src/lib.rs"));
}

/// A pushed tree reads back through `contents`, `readme` and `raw`. Pushed
/// packs are kept as packs, so `raw` reads the blob from the pack.
#[tokio::test(flavor = "multi_thread")]
async fn pushed_files_read_back_through_contents_readme_and_raw() {
    use klotho_core::{Action, Actor};
    use klotho_git::{Contents, EntryType};

    let running = common::start(Duration::from_secs(5)).await;
    let work = tempfile::tempdir().unwrap();
    let (work_path, url) = (work.path().to_owned(), running.url("/alice/demo.git"));
    blocking(move || {
        let local = work_path.join("local");
        std::fs::create_dir_all(local.join("docs")).unwrap();
        git(&local, &["init", "-q"]);
        std::fs::write(local.join("Readme.markdown"), "# Demo\n").unwrap();
        std::fs::write(local.join("docs").join("guide.txt"), "guide\n").unwrap();
        std::fs::write(local.join("data.bin"), [0u8, 1, 2, 3]).unwrap();
        git(&local, &["add", "."]);
        git(&local, &["commit", "-q", "-m", "files"]);
        git(&local, &["push", "-q", &url, "HEAD:refs/heads/main"]);
    })
    .await;

    let core = &running.core;
    let repo = core.repo_for(&Actor::Anonymous, "alice", "demo", Action::Read).await.unwrap();
    let Contents::Dir(root) = core.contents(&repo, None, "", None, None).await.unwrap() else { panic!() };
    let names: Vec<_> = root.entries.iter().map(|entry| (entry.name.as_str(), entry.kind)).collect();
    assert_eq!(
        names,
        [("docs", EntryType::Dir), ("Readme.markdown", EntryType::File), ("data.bin", EntryType::File)]
    );
    let Contents::File(data) = core.contents(&repo, Some("main"), "data.bin", None, None).await.unwrap()
    else {
        panic!()
    };
    assert!(data.binary && data.size == 4);

    let readme = core.readme(&repo, None, "").await.unwrap().unwrap();
    assert_eq!(readme.text.as_deref(), Some("# Demo\n"));

    let mut raw = core.raw(&repo, None, "docs/guide.txt").await.unwrap();
    assert_eq!(raw.size, 6);
    let mut bytes = Vec::new();
    while let Some(chunk) = raw.chunks.recv().await {
        bytes.extend(chunk.unwrap());
    }
    assert_eq!(bytes, b"guide\n");
    let missing = core.raw(&repo, None, "docs").await.err().unwrap();
    assert!(matches!(missing, klotho_core::Error::Git(klotho_git::Error::PathNotFound(_))));

    let git = core.store().open(repo.id).unwrap();
    let (_, reader) = klotho_git::open_file(&git, None, "docs/guide.txt").unwrap();
    assert!(!reader.in_memory(), "a blob stored whole in a pack streams");
}
