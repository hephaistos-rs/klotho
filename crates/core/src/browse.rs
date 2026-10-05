//! Reading a repository: refs, history and contents, for the web UI and the API alike
//! (FR-API-004). Every method takes a [`Repo`] from [`Core::repo_for`], so the
//! caller has already been through `authorize()`.

use std::io::Read;

pub use klotho_git::{
    Annotation, BranchInfo, CommitInfo, Contents, DirListing, EntryType, FileInfo, GitTime, LinkInfo, Page,
    RefKind, RefTarget, RepoInfo, Resolved, Signature, SubmoduleInfo, TagInfo, TreeEntry,
};
use tokio::sync::{mpsc, oneshot};

use crate::markdown::{self, LinkBase};
use crate::{Core, Error, Repo, Result};

/// How much of a raw file is read at a time, and how many reads may wait in
/// memory for a slow client: at most 256 KiB per download (NFR-PERF-013).
const RAW_CHUNK: usize = 64 * 1024;
const RAW_QUEUE: usize = 4;

/// Page sizes (FR-API-020).
pub const DEFAULT_PAGE: u32 = 30;
pub const MAX_PAGE: u32 = 100;

/// `limit`, or the default, kept within 1 to [`MAX_PAGE`].
pub fn page_size(limit: Option<u32>) -> usize {
    limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE) as usize
}

/// What to list in [`Core::commits`].
#[derive(Debug, Default, Clone)]
pub struct LogQuery {
    /// A branch, tag or commit ID; the default branch if `None`.
    pub rev: Option<String>,
    /// Only commits that changed this path.
    pub path: String,
    /// From the previous page's `next`. It overrides `rev`.
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

/// A README, rendered by [`Core::readme_html`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct Readme {
    pub path: String,
    pub name: String,
    /// Sanitised HTML, safe to insert into a page.
    pub html: String,
    /// The commit it was read from.
    #[serde(rename = "ref")]
    pub target: RefTarget,
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

/// The chunks of a raw file, in order; an `Err` ends it early.
pub type RawChunks = mpsc::Receiver<std::io::Result<Vec<u8>>>;

/// A file being read for download.
pub struct RawFile {
    /// The commit it was read from.
    pub target: RefTarget,
    pub size: u64,
    /// The content, in order. An `Err` ends it early.
    pub chunks: RawChunks,
}

impl Core {
    /// The branch HEAD points at, and whether the repository has any commits.
    pub async fn repo_info(&self, repo: &Repo) -> Result<RepoInfo> {
        self.with_git(repo, RepoInfo::read).await
    }

    /// The commit a `?ref=` (branch, tag or commit ID) names.
    pub async fn resolve_ref(&self, repo: &Repo, rev: Option<&str>) -> Result<RefTarget> {
        let rev = rev.map(str::to_owned);
        self.with_git(repo, move |git| klotho_git::resolve_ref(git, rev.as_deref())).await
    }

    /// Splits a UI `ref/path` spec, trying the longest branch or tag first.
    pub async fn resolve_spec(&self, repo: &Repo, spec: &str) -> Result<Resolved> {
        let spec = spec.to_owned();
        self.with_git(repo, move |git| klotho_git::resolve(git, &spec)).await
    }

    /// Branches by name, with their latest commit (FR-UI-007).
    pub async fn branches(
        &self,
        repo: &Repo,
        after: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Page<BranchInfo>> {
        let (after, limit) = (after.map(str::to_owned), page_size(limit));
        self.with_git(repo, move |git| klotho_git::branches(git, after.as_deref(), limit)).await
    }

    pub async fn branch(&self, repo: &Repo, name: &str) -> Result<BranchInfo> {
        let name = name.to_owned();
        self.with_git(repo, move |git| klotho_git::branch(git, &name)).await
    }

    /// Tags by name, with the commit each comes to (FR-UI-007).
    pub async fn tags(&self, repo: &Repo, after: Option<&str>, limit: Option<u32>) -> Result<Page<TagInfo>> {
        let (after, limit) = (after.map(str::to_owned), page_size(limit));
        self.with_git(repo, move |git| klotho_git::tags(git, after.as_deref(), limit)).await
    }

    pub async fn tag(&self, repo: &Repo, name: &str) -> Result<TagInfo> {
        let name = name.to_owned();
        self.with_git(repo, move |git| klotho_git::tag(git, &name)).await
    }

    /// What is at `path` in `rev`: a directory listing (paged), a file with its
    /// text if small and not binary, a symlink or a submodule (FR-UI-001, 003).
    pub async fn contents(
        &self,
        repo: &Repo,
        rev: Option<&str>,
        path: &str,
        cursor: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Contents> {
        let (rev, path, cursor) = (rev.map(str::to_owned), path.to_owned(), cursor.map(str::to_owned));
        let limit = page_size(limit);
        self.with_git(repo, move |git| {
            klotho_git::contents(git, rev.as_deref(), &path, cursor.as_deref(), limit)
        })
        .await
    }

    /// The README in the directory `dir` (the root if empty) of `rev` (FR-UI-002).
    pub async fn readme(&self, repo: &Repo, rev: Option<&str>, dir: &str) -> Result<Option<FileInfo>> {
        let (rev, dir) = (rev.map(str::to_owned), dir.to_owned());
        self.with_git(repo, move |git| klotho_git::readme(git, rev.as_deref(), &dir)).await
    }

    /// The README in `dir` of `rev`, rendered for reading (FR-UI-002). Markdown
    /// becomes sanitised HTML whose relative links and images point at the
    /// repository's own pages; any other README is shown as preformatted text.
    /// `None` when there is no README, or it's binary or too large to show.
    pub async fn readme_html(&self, repo: &Repo, rev: Option<&str>, dir: &str) -> Result<Option<Readme>> {
        let target = self.resolve_ref(repo, rev).await?;
        let Some(file) = self.readme(repo, Some(&target.commit), dir).await? else {
            return Ok(None);
        };
        let Some(text) = &file.text else {
            return Ok(None);
        };
        let lower = file.name.to_ascii_lowercase();
        let is_markdown = [".md", ".markdown", ".mdown", ".mkd"].iter().any(|ext| lower.ends_with(ext));
        let html = if is_markdown {
            let full_name = repo.full_name();
            let base = LinkBase {
                blob: self.urls.repo_page(&full_name, "blob", &target.name),
                raw: self.urls.repo_page(&full_name, "raw", &target.name),
                dir: dir.trim_matches('/').to_owned(),
            };
            markdown::render(text, Some(&base))
        } else {
            format!("<pre>{}</pre>", escape_html(text))
        };
        Ok(Some(Readme { path: file.path.clone(), name: file.name.clone(), html, target }))
    }

    /// The bytes of the file at `path` in `rev`, streamed (NFR-PERF-013). A
    /// blocking task reads ahead at most a few chunks; it stops when the
    /// receiver is dropped, e.g. because the client went away.
    pub async fn raw(&self, repo: &Repo, rev: Option<&str>, path: &str) -> Result<RawFile> {
        let (rev, path, id) = (rev.map(str::to_owned), path.to_owned(), repo.id);
        let store = self.store.clone();
        let (opened_tx, opened) = oneshot::channel();
        let (tx, chunks) = mpsc::channel(RAW_QUEUE);
        tokio::task::spawn_blocking(move || {
            let opened = store.open(id).and_then(|git| klotho_git::open_file(&git, rev.as_deref(), &path));
            let mut reader = match opened {
                Ok((target, reader)) => {
                    let _ = opened_tx.send(Ok((target, reader.size())));
                    reader
                }
                Err(err) => {
                    let _ = opened_tx.send(Err(err));
                    return;
                }
            };
            loop {
                let mut chunk = vec![0; RAW_CHUNK];
                let item = match reader.read(&mut chunk) {
                    Ok(0) => return,
                    Ok(n) => {
                        chunk.truncate(n);
                        Ok(chunk)
                    }
                    Err(err) => Err(err),
                };
                let failed = item.is_err();
                if tx.blocking_send(item).is_err() || failed {
                    return;
                }
            }
        });
        let (target, size) = opened.await.map_err(|_| Error::Internal("raw reader stopped".to_owned()))??;
        Ok(RawFile { target, size, chunks })
    }

    /// The commit log, newest first, a page at a time (FR-UI-005, NFR-PERF-014).
    pub async fn commits(&self, repo: &Repo, query: LogQuery) -> Result<Page<CommitInfo>> {
        let limit = page_size(query.limit);
        self.with_git(repo, move |git| {
            klotho_git::log(git, query.rev.as_deref(), &query.path, query.cursor.as_deref(), limit)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::core;
    use crate::{Action, Actor, Error, NewUser};

    #[test]
    fn page_sizes_stay_within_bounds() {
        assert_eq!(page_size(None), 30);
        assert_eq!(page_size(Some(0)), 1);
        assert_eq!(page_size(Some(1000)), 100);
    }

    #[tokio::test]
    async fn an_empty_repository_reads_as_empty() {
        let (_dir, core) = core().await;
        core.create_user(NewUser::named("alice")).await.unwrap();
        core.create_repo("alice", "demo", false).await.unwrap();
        let repo = core.repo_for(&Actor::Anonymous, "alice", "demo", Action::Read).await.unwrap();

        assert!(core.commits(&repo, LogQuery::default()).await.unwrap().items.is_empty());
        assert!(core.branches(&repo, None, None).await.unwrap().items.is_empty());
        assert!(core.tags(&repo, None, None).await.unwrap().items.is_empty());
        let missing = core.resolve_ref(&repo, Some("main")).await.unwrap_err();
        assert!(matches!(missing, Error::Git(klotho_git::Error::RevisionNotFound(_))));
        assert!(matches!(
            core.resolve_spec(&repo, "").await,
            Err(Error::Git(klotho_git::Error::RevisionNotFound(_)))
        ));
    }
}
