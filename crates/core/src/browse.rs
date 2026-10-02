//! Reading a repository's refs and history, for the web UI and the API alike
//! (FR-API-004). Every method takes a [`Repo`] from [`Core::repo_for`], so the
//! caller has already been through `authorize()`.

use klotho_git::{BranchInfo, CommitInfo, Page, RefTarget, Resolved, TagInfo};

use crate::{Core, Repo, Result};

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

impl Core {
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
