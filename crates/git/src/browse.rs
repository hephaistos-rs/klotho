//! Read-only views of a repository, shaped for the web UI and API.

use gix::objs::tree::EntryKind;
use serde::Serialize;

use crate::{Error, RepoName, Result};

#[derive(Debug, Serialize)]
pub struct RepoInfo {
    pub name: String,
    /// The branch HEAD points at, e.g. `main`, even if it has no commits yet.
    pub default_branch: Option<String>,
    pub empty: bool,
    pub refs: Vec<RefInfo>,
}

#[derive(Debug, Serialize)]
pub struct RefInfo {
    pub name: String,
    pub target: String,
}

#[derive(Debug, Serialize)]
pub struct CommitInfo {
    pub id: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub author_email: String,
    /// Seconds since the Unix epoch.
    pub author_time: i64,
    pub summary: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct TreeEntryInfo {
    pub name: String,
    pub kind: &'static str,
    pub id: String,
}

impl RepoInfo {
    pub fn read(repo: &gix::Repository, name: &RepoName) -> Result<Self> {
        let head = repo.head().map_err(Error::git)?;
        let default_branch = head.referent_name().map(|name| name.shorten().to_string());

        let mut refs = Vec::new();
        let platform = repo.references().map_err(Error::git)?;
        for reference in platform.all().map_err(Error::git)? {
            let mut reference = reference.map_err(Error::Git)?;
            let target = reference.peel_to_id().map_err(Error::git)?;
            refs.push(RefInfo { name: reference.name().as_bstr().to_string(), target: target.to_string() });
        }

        Ok(Self { name: name.to_string(), default_branch, empty: head.is_unborn(), refs })
    }
}

impl CommitInfo {
    /// Up to `limit` commits reachable from `rev`, newest first.
    pub fn log(repo: &gix::Repository, rev: &str, limit: usize) -> Result<Vec<Self>> {
        let commit = resolve_commit(repo, rev)?;
        let walk = repo.rev_walk([commit.id]).all().map_err(Error::git)?;
        walk.take(limit)
            .map(|info| {
                let commit = info.map_err(Error::git)?.object().map_err(Error::git)?;
                Self::from_commit(&commit)
            })
            .collect()
    }

    fn from_commit(commit: &gix::Commit<'_>) -> Result<Self> {
        let author = commit.author().map_err(Error::git)?;
        let message = commit.message_raw().map_err(Error::git)?;
        let summary = commit.message().map_err(Error::git)?.summary();
        Ok(Self {
            id: commit.id.to_string(),
            parents: commit.parent_ids().map(|id| id.to_string()).collect(),
            author_name: author.name.to_string(),
            author_email: author.email.to_string(),
            author_time: author.seconds(),
            summary: summary.to_string(),
            message: message.to_string(),
        })
    }
}

impl TreeEntryInfo {
    /// The entries of the directory at `path` (empty for the root) in `rev`.
    pub fn list(repo: &gix::Repository, rev: &str, path: &str) -> Result<Vec<Self>> {
        let root = resolve_commit(repo, rev)?.tree().map_err(Error::git)?;
        let tree = if path.is_empty() {
            root
        } else {
            let entry = root
                .lookup_entry_by_path(path)
                .map_err(Error::git)?
                .filter(|entry| entry.mode().is_tree())
                .ok_or_else(|| Error::PathNotFound(path.to_owned()))?;
            entry.object().map_err(Error::git)?.try_into_tree().map_err(Error::git)?
        };

        tree.iter()
            .map(|entry| {
                let entry = entry.map_err(Error::git)?;
                Ok(Self {
                    name: entry.filename().to_string(),
                    kind: kind_name(entry.mode().kind()),
                    id: entry.oid().to_string(),
                })
            })
            .collect()
    }
}

/// The raw contents of the file at `path` in `rev`.
pub fn read_blob(repo: &gix::Repository, rev: &str, path: &str) -> Result<Vec<u8>> {
    let root = resolve_commit(repo, rev)?.tree().map_err(Error::git)?;
    let entry = root
        .lookup_entry_by_path(path)
        .map_err(Error::git)?
        .filter(|entry| entry.mode().is_blob())
        .ok_or_else(|| Error::PathNotFound(path.to_owned()))?;
    let blob = entry.object().map_err(Error::git)?;
    Ok(blob.detach().data)
}

fn resolve_commit<'repo>(repo: &'repo gix::Repository, rev: &str) -> Result<gix::Commit<'repo>> {
    let not_found = || Error::RevisionNotFound(rev.to_owned());
    let id = repo.rev_parse_single(rev).map_err(|_| not_found())?;
    let object = id.object().map_err(Error::git)?;
    object.peel_to_commit().map_err(|_| not_found())
}

fn kind_name(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Tree => "tree",
        EntryKind::Blob => "blob",
        EntryKind::BlobExecutable => "executable",
        EntryKind::Link => "symlink",
        EntryKind::Commit => "submodule",
    }
}
