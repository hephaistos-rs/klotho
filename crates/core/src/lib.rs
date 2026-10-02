//! Klotho's domain and data layer: names, permissions, services and the metadata store. No HTTP.
//!
//! [`Core`] is the entry point. The web UI and the API both go through its
//! methods, never through SQL or `klotho-git` directly, so they can't drift
//! apart (FR-API-004).

pub mod access;
mod auth;
pub mod browse;
pub mod config;
pub mod db;
mod error;
pub mod names;
mod owners;
mod repos;
mod secret;
mod urls;

use sqlx::SqlitePool;

pub use access::{Action, Actor, Scope, Scopes};
pub use auth::{INVITE_LIFETIME, NewToken, NewUser, TokenInfo, User};
pub use browse::{LogQuery, RawFile};
pub use error::{Error, Result};
use klotho_git::RepoStore;
pub use owners::{Owner, OwnerKind};
pub use repos::{Repo, StorageReport};
pub use urls::Urls;

use crate::config::{AuthConfig, DataPaths};

/// The services, sharing one database pool and repository store. Cheap to clone.
#[derive(Clone)]
pub struct Core {
    db: SqlitePool,
    store: RepoStore,
    urls: Urls,
    auth: AuthConfig,
}

impl Core {
    /// Opens the database (running migrations) and the repository store.
    pub async fn open(paths: &DataPaths, urls: Urls, auth: AuthConfig) -> Result<Self> {
        let db = db::open(&paths.database).await?;
        let store = RepoStore::new(&paths.repositories)?;
        Ok(Self { db, store, urls, auth })
    }

    pub fn urls(&self) -> &Urls {
        &self.urls
    }

    /// The longest a web session can last (FR-AUTH-041).
    pub fn session_max(&self) -> std::time::Duration {
        self.auth.session_max()
    }

    /// The repository store, for serving git transport.
    pub fn store(&self) -> &RepoStore {
        &self.store
    }

    /// Readiness (NFR-OPS-022): the database answers and the storage root exists.
    pub async fn ready(&self) -> Result<()> {
        sqlx::query!("SELECT 1 AS one").fetch_one(&self.db).await?;
        if !self.store.root().is_dir() {
            return Err(Error::StorageUnavailable(self.store.root().display().to_string()));
        }
        Ok(())
    }

    /// Runs blocking `klotho-git` work on the blocking thread pool (NFR-PERF-022).
    pub(crate) async fn blocking<T: Send + 'static>(
        &self,
        f: impl FnOnce(&RepoStore) -> klotho_git::Result<T> + Send + 'static,
    ) -> Result<T> {
        let store = self.store.clone();
        Ok(tokio::task::spawn_blocking(move || f(&store)).await??)
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;

    /// A `Core` on a fresh database and storage root in a temporary directory.
    pub async fn core() -> (tempfile::TempDir, Core) {
        core_with(AuthConfig::default()).await
    }

    pub async fn core_with(auth: AuthConfig) -> (tempfile::TempDir, Core) {
        let dir = tempfile::tempdir().unwrap();
        let paths = config::StorageConfig::default().resolve(dir.path());
        let core = Core::open(&paths, Urls::new("http://localhost:3000").unwrap(), auth).await.unwrap();
        (dir, core)
    }
}
