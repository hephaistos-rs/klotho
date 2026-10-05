//! Klotho's domain and data layer: names, permissions, services and the metadata store. No HTTP.
//!
//! [`Core`] is the entry point. The web UI and the API both go through its
//! methods, never through SQL or `klotho-git` directly, so they can't drift
//! apart (FR-API-004). The services hold no SQL either: every query is in
//! [`meta`], behind the `MetaStore` trait.

pub mod access;
mod auth;
pub mod browse;
pub mod config;
mod error;
pub mod markdown;
mod meta;
pub mod names;
mod owners;
mod repos;
mod secret;
mod urls;

pub use access::{Action, Actor, Scope, Scopes};
pub use auth::{INVITE_LIFETIME, NewToken, NewUser, TokenInfo, User};
pub use browse::{LogQuery, RawFile, Readme};
pub use error::{Error, Result};
use klotho_git::RepoStore;
pub use owners::{Owner, OwnerKind};
pub use repos::{Repo, StorageReport};
pub use urls::{Urls, encode_path};

use std::sync::Arc;

use crate::config::{AuthConfig, DataPaths};
use crate::meta::{MetaStore, SqliteMeta};

/// The services, sharing one metadata store and repository store. Cheap to clone.
#[derive(Clone)]
pub struct Core {
    meta: Arc<dyn MetaStore>,
    store: RepoStore,
    urls: Urls,
    auth: AuthConfig,
}

impl Core {
    /// Opens the database (running migrations) and the repository store.
    pub async fn open(paths: &DataPaths, urls: Urls, auth: AuthConfig) -> Result<Self> {
        let meta = SqliteMeta::open(&paths.database).await?;
        Self::with_meta(Arc::new(meta), paths, urls, auth)
    }

    fn with_meta(meta: Arc<dyn MetaStore>, paths: &DataPaths, urls: Urls, auth: AuthConfig) -> Result<Self> {
        let store = RepoStore::new(&paths.repositories)?;
        Ok(Self { meta, store, urls, auth })
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
        self.meta.ping().await?;
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
        let (dir, core, _) = core_and_pool_with(auth).await;
        (dir, core)
    }

    /// [`core`], and the SQLite pool under it, for tests that set up states
    /// the services can't, such as a session from long ago.
    pub async fn core_and_pool() -> (tempfile::TempDir, Core, sqlx::SqlitePool) {
        core_and_pool_with(AuthConfig::default()).await
    }

    async fn core_and_pool_with(auth: AuthConfig) -> (tempfile::TempDir, Core, sqlx::SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let paths = config::StorageConfig::default().resolve(dir.path());
        let meta = SqliteMeta::open(&paths.database).await.unwrap();
        let pool = meta.pool().clone();
        let urls = Urls::new("http://localhost:3000").unwrap();
        let core = Core::with_meta(Arc::new(meta), &paths, urls, auth).unwrap();
        (dir, core, pool)
    }
}
