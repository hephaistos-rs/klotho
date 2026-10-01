use std::collections::HashSet;

use klotho_git::{RepoId, RepoStore};
use serde::Serialize;

use crate::error::is_unique_violation;
use crate::names::{OwnerName, RepoName};
use crate::{Core, Error, Owner, OwnerKind, Result, db};

/// A repository as the metadata store knows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Repo {
    pub id: RepoId,
    pub owner: Owner,
    /// The display form, as typed (FR-NAME-020).
    pub name: String,
    pub created_at: jiff::Timestamp,
}

impl Repo {
    /// `Owner/Repo`, with display names.
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner.name, self.name)
    }
}

/// Where disk and metadata disagree (FR-STOR-020).
#[derive(Debug, Default, Serialize)]
pub struct StorageReport {
    /// Bare repositories under the storage root that no repository row points at,
    /// as `/`-separated paths relative to the root. They can be adopted or deleted.
    pub unadopted: Vec<String>,
    /// Repositories whose directory is missing.
    pub missing: Vec<Repo>,
}

/// A row from the `repo_select!` query.
macro_rules! repo_from_row {
    ($row:expr) => {
        Repo {
            id: $row.id,
            owner: Owner {
                id: $row.owner_id,
                name: $row.owner_name,
                kind: OwnerKind::from_db(&$row.owner_kind),
            },
            name: $row.name,
            created_at: db::parse_time(&$row.created_at),
        }
    };
}

impl Core {
    /// Creates an empty repository (FR-REPO-001, FR-STOR-005). The database row
    /// and the directory are created together or not at all:
    ///
    /// 1. insert the row in a transaction; the unique index on
    ///    `(owner_id, name_key)` rejects duplicates, even under concurrency;
    /// 2. initialise the repository in a temporary directory and rename it into
    ///    place under the new ID;
    /// 3. commit. If that fails, the directory is set aside again.
    pub async fn create_repo(&self, owner: &str, name: &str) -> Result<Repo> {
        let name = RepoName::parse_new(name)?;
        let owner = self.find_owner(owner).await?;
        let full_name = format!("{}/{}", owner.name, name);
        let (display, key, now) = (name.as_str(), name.key(), db::now());

        let mut tx = self.db.begin().await?;
        let id = sqlx::query_scalar!(
            r#"INSERT INTO repositories (owner_id, name, name_key, created_at) VALUES (?, ?, ?, ?) RETURNING id AS "id!""#,
            owner.id,
            display,
            key,
            now,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|err| if is_unique_violation(&err) { Error::RepoExists(full_name.clone()) } else { err.into() })?;

        self.blocking(move |store| create_dir(store, id)).await?;
        if let Err(err) = tx.commit().await {
            let _ = self.blocking(move |store| store.set_aside(id)).await;
            return Err(err.into());
        }
        Ok(Repo { id, owner, name: name.to_string(), created_at: db::parse_time(&now) })
    }

    /// Looks a repository up by URL segments: case-insensitive, `.git` optional
    /// (FR-NAME-021, 040). A missing owner is reported as a missing repository.
    pub async fn find_repo(&self, owner: &str, name: &str) -> Result<Repo> {
        let not_found = || Error::RepoNotFound(format!("{owner}/{name}"));
        let (Ok(owner_name), Ok(repo_name)) = (OwnerName::parse_lookup(owner), RepoName::parse_lookup(name))
        else {
            return Err(not_found());
        };
        let (owner_key, repo_key) = (owner_name.key(), repo_name.key());
        let row = sqlx::query!(
            r#"SELECT r.id AS "id!", r.name, r.created_at, o.id AS "owner_id!", o.name AS owner_name, o.kind AS owner_kind
               FROM repositories r JOIN owners o ON o.id = r.owner_id
               WHERE o.name_key = ? AND r.name_key = ?"#,
            owner_key,
            repo_key,
        )
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(not_found)?;
        Ok(repo_from_row!(row))
    }

    /// One page of an owner's repositories, ordered by name key. `after` is the
    /// key of the last repository on the previous page.
    pub async fn list_repos(&self, owner: &str, after: Option<&str>, limit: u32) -> Result<Vec<Repo>> {
        let owner = self.find_owner(owner).await?;
        let after = after.unwrap_or("");
        let limit = i64::from(limit);
        let rows = sqlx::query!(
            r#"SELECT r.id AS "id!", r.name, r.created_at, o.id AS "owner_id!", o.name AS owner_name, o.kind AS owner_kind
               FROM repositories r JOIN owners o ON o.id = r.owner_id
               WHERE r.owner_id = ? AND r.name_key > ?
               ORDER BY r.name_key LIMIT ?"#,
            owner.id,
            after,
            limit,
        )
        .fetch_all(&self.db)
        .await?;
        Ok(rows.into_iter().map(|row| repo_from_row!(row)).collect())
    }

    /// Compares the storage root with the database (FR-STOR-020).
    pub async fn storage_report(&self) -> Result<StorageReport> {
        let rows = sqlx::query!(
            r#"SELECT r.id AS "id!", r.name, r.created_at, o.id AS "owner_id!", o.name AS owner_name, o.kind AS owner_kind
               FROM repositories r JOIN owners o ON o.id = r.owner_id ORDER BY r.id"#
        )
        .fetch_all(&self.db)
        .await?;
        let repos: Vec<Repo> = rows.into_iter().map(|row| repo_from_row!(row)).collect();

        let ids: Vec<RepoId> = repos.iter().map(|repo| repo.id).collect();
        let (on_disk, present) = self
            .blocking(move |store| {
                let present: HashSet<RepoId> = ids.into_iter().filter(|&id| store.exists(id)).collect();
                Ok((store.scan()?, present))
            })
            .await?;

        let known: HashSet<String> = repos.iter().map(|repo| relative_path(repo.id)).collect();
        Ok(StorageReport {
            unadopted: on_disk.into_iter().filter(|path| !known.contains(path)).collect(),
            missing: repos.into_iter().filter(|repo| !present.contains(&repo.id)).collect(),
        })
    }

    /// Registers the unadopted repository at `path` (from [`Self::storage_report`])
    /// under `owner`, moving it to its ID-based location (FR-REPO-035). The name
    /// defaults to the directory name without `.git`.
    pub async fn adopt(&self, path: &str, owner: &str, name: Option<&str>) -> Result<Repo> {
        if !self.storage_report().await?.unadopted.iter().any(|p| p == path) {
            return Err(Error::NotUnadopted(path.to_owned()));
        }
        let default_name = path.rsplit('/').next().unwrap_or(path);
        let default_name = default_name.strip_suffix(".git").unwrap_or(default_name);
        let name = RepoName::parse_new(name.unwrap_or(default_name))?;
        let owner = self.find_owner(owner).await?;
        let full_name = format!("{}/{}", owner.name, name);
        let (display, key, now) = (name.as_str(), name.key(), db::now());

        let mut tx = self.db.begin().await?;
        let id = sqlx::query_scalar!(
            r#"INSERT INTO repositories (owner_id, name, name_key, created_at) VALUES (?, ?, ?, ?) RETURNING id AS "id!""#,
            owner.id,
            display,
            key,
            now,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|err| if is_unique_violation(&err) { Error::RepoExists(full_name.clone()) } else { err.into() })?;

        let from = path.to_owned();
        self.blocking(move |store| {
            if store.exists(id) {
                store.set_aside(id)?;
            }
            store.adopt(&from, id)
        })
        .await?;
        if let Err(err) = tx.commit().await {
            let back = path.to_owned();
            let _ = self.blocking(move |store| store.unadopt(id, &back)).await;
            return Err(err.into());
        }
        tracing::info!(path, repo = %full_name, id, "adopted repository");
        Ok(Repo { id, owner, name: name.to_string(), created_at: db::parse_time(&now) })
    }

    /// Deletes the unadopted repository at `path` (FR-REPO-035).
    pub async fn delete_unadopted(&self, path: &str) -> Result<()> {
        if !self.storage_report().await?.unadopted.iter().any(|p| p == path) {
            return Err(Error::NotUnadopted(path.to_owned()));
        }
        let target = path.to_owned();
        self.blocking(move |store| store.delete_relative(&target)).await?;
        tracing::info!(path, "deleted unadopted repository");
        Ok(())
    }

    /// Runs `f` on the opened git repository, on the blocking pool.
    pub async fn with_git<T: Send + 'static>(
        &self,
        repo: &Repo,
        f: impl FnOnce(&gix::Repository) -> klotho_git::Result<T> + Send + 'static,
    ) -> Result<T> {
        let id = repo.id;
        self.blocking(move |store| f(&store.open(id)?)).await
    }
}

/// Creates the directory for a new ID. A directory already there belongs to no
/// committed row (the ID was just handed out), so it's left over from a create
/// that crashed before committing: set it aside and carry on (FR-STOR-021).
fn create_dir(store: &RepoStore, id: RepoId) -> klotho_git::Result<()> {
    if store.exists(id) {
        let moved = store.set_aside(id)?;
        tracing::warn!(id, moved = %moved.display(), "set aside a leftover directory for a new repository ID");
    }
    store.create(id)
}

fn relative_path(id: RepoId) -> String {
    RepoStore::relative_path(id).to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::core;

    #[tokio::test]
    async fn create_keeps_display_name_and_finds_case_insensitively() {
        let (_dir, core) = core().await;
        core.create_user("Alice").await.unwrap();
        let repo = core.create_repo("alice", "MyRepo").await.unwrap();
        assert_eq!(repo.full_name(), "Alice/MyRepo");
        assert!(core.store().exists(repo.id));

        for (owner, name) in [("Alice", "MyRepo"), ("ALICE", "myrepo"), ("alice", "myrepo.git")] {
            assert_eq!(core.find_repo(owner, name).await.unwrap(), repo);
        }
        assert!(matches!(core.find_repo("alice", "other").await, Err(Error::RepoNotFound(_))));
        assert!(matches!(core.find_repo("nobody", "MyRepo").await, Err(Error::RepoNotFound(_))));
    }

    #[tokio::test]
    async fn duplicate_names_conflict_regardless_of_case() {
        let (_dir, core) = core().await;
        core.create_user("alice").await.unwrap();
        core.create_repo("alice", "demo").await.unwrap();
        assert!(matches!(core.create_repo("alice", "Demo").await, Err(Error::RepoExists(_))));
    }

    #[tokio::test]
    async fn the_same_name_under_different_owners_is_fine() {
        let (_dir, core) = core().await;
        core.create_user("alice").await.unwrap();
        core.create_user("bob").await.unwrap();
        let a = core.create_repo("alice", "demo").await.unwrap();
        let b = core.create_repo("bob", "demo").await.unwrap();
        assert_ne!(a.id, b.id);
    }

    #[tokio::test]
    async fn new_names_ending_in_git_are_rejected() {
        let (_dir, core) = core().await;
        core.create_user("alice").await.unwrap();
        assert!(matches!(core.create_repo("alice", "demo.git").await, Err(Error::InvalidName(_))));
    }

    #[tokio::test]
    async fn windows_device_names_are_ordinary_repositories() {
        let (_dir, core) = core().await;
        core.create_user("alice").await.unwrap();
        for name in ["con", "nul", "aux", "com1", "LPT1"] {
            let repo = core.create_repo("alice", name).await.unwrap();
            core.with_git(&repo, klotho_git::RepoInfo::read).await.unwrap();
        }
    }

    #[tokio::test]
    async fn concurrent_creates_of_one_name_have_exactly_one_winner() {
        let (_dir, core) = core().await;
        core.create_user("alice").await.unwrap();
        let attempts: Vec<_> = (0..50)
            .map(|_| {
                let core = core.clone();
                tokio::spawn(async move { core.create_repo("alice", "race").await })
            })
            .collect();
        let mut created = 0;
        for attempt in attempts {
            match attempt.await.unwrap() {
                Ok(_) => created += 1,
                Err(Error::RepoExists(_)) => {}
                Err(other) => panic!("unexpected error: {other}"),
            }
        }
        assert_eq!(created, 1);
        let report = core.storage_report().await.unwrap();
        assert!(report.unadopted.is_empty() && report.missing.is_empty(), "{report:?}");
    }

    #[tokio::test]
    async fn a_leftover_directory_for_a_new_id_is_set_aside() {
        let (_dir, core) = core().await;
        core.create_user("alice").await.unwrap();
        // A crash after creating the directory but before committing leaves a
        // directory for ID 1 and no row, so the next create gets ID 1 again.
        core.store().create(1).unwrap();
        let repo = core.create_repo("alice", "demo").await.unwrap();
        assert_eq!(repo.id, 1);
        assert!(core.storage_report().await.unwrap().unadopted.is_empty());
    }

    #[tokio::test]
    async fn adopt_and_delete_unadopted_repositories() {
        let (_dir, core) = core().await;
        core.create_user("alice").await.unwrap();
        gix::init_bare(core.store().root().join("Legacy.git")).unwrap();
        gix::init_bare(core.store().root().join("junk.git")).unwrap();

        let report = core.storage_report().await.unwrap();
        assert_eq!(report.unadopted, vec!["Legacy.git".to_owned(), "junk.git".to_owned()]);

        let repo = core.adopt("Legacy.git", "alice", None).await.unwrap();
        assert_eq!(repo.full_name(), "alice/Legacy");
        assert!(core.store().exists(repo.id));
        assert!(matches!(core.adopt("Legacy.git", "alice", None).await, Err(Error::NotUnadopted(_))));

        core.delete_unadopted("junk.git").await.unwrap();
        assert!(core.storage_report().await.unwrap().unadopted.is_empty());
    }

    #[tokio::test]
    async fn a_repository_without_its_directory_is_reported_missing() {
        let (_dir, core) = core().await;
        core.create_user("alice").await.unwrap();
        let repo = core.create_repo("alice", "demo").await.unwrap();
        std::fs::remove_dir_all(core.store().path(repo.id)).unwrap();
        assert_eq!(core.storage_report().await.unwrap().missing, vec![repo]);
    }

    #[tokio::test]
    async fn listing_is_paged_by_name_key() {
        let (_dir, core) = core().await;
        core.create_user("alice").await.unwrap();
        for name in ["b", "A", "c"] {
            core.create_repo("alice", name).await.unwrap();
        }
        let first: Vec<_> =
            core.list_repos("alice", None, 2).await.unwrap().into_iter().map(|r| r.name).collect();
        assert_eq!(first, ["A", "b"]);
        let rest: Vec<_> =
            core.list_repos("alice", Some("b"), 2).await.unwrap().into_iter().map(|r| r.name).collect();
        assert_eq!(rest, ["c"]);
    }
}
