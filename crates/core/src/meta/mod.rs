//! The metadata store (FR-STOR-001, 002): every database query Klotho makes,
//! behind one trait. The services in `auth.rs`, `repos.rs` and `owners.rs` hold
//! no SQL, so a second database (PostgreSQL, Phase 10) is a second
//! implementation of [`MetaStore`], not a change to the services.
//!
//! The trait speaks the domain's language: names arrive already parsed and
//! keyed, times cross it as [`Timestamp`] (each database stores them as suits
//! it), and a unique-key conflict comes back as the matching domain error
//! ([`Error::OwnerExists`](crate::Error), …). Each implementation checks its
//! queries against its own database at compile time with `sqlx::query!`.
//!
//! Methods return boxed futures, so the store can be chosen at startup and held
//! as `Arc<dyn MetaStore>`.

mod sqlite;

use std::future::Future;
use std::pin::Pin;

use jiff::Timestamp;
use klotho_git::RepoId;

use crate::{Owner, Repo, Result, Scopes, TokenInfo, User};

pub(crate) use sqlite::SqliteMeta;

/// What every store method returns.
pub(crate) type Fut<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// Reads, and writes that are one statement each.
pub(crate) trait MetaStore: Send + Sync {
    /// Whether the database answers (NFR-OPS-022).
    fn ping(&self) -> Fut<'_, ()>;
    /// A transaction for writes that must happen together. Dropping it without
    /// [`WriteTx::commit`] rolls it back.
    fn begin_write(&self) -> Fut<'_, Box<dyn WriteTx>>;

    fn find_owner<'a>(&'a self, name_key: &'a str) -> Fut<'a, Option<Owner>>;
    fn find_user<'a>(&'a self, name_key: &'a str) -> Fut<'a, Option<UserRecord>>;

    fn create_invite<'a>(&'a self, hash: &'a [u8], now: Timestamp, expires_at: Timestamp) -> Fut<'a, ()>;

    fn create_session<'a>(&'a self, hash: &'a [u8], user_id: i64, now: Timestamp) -> Fut<'a, ()>;
    fn find_session<'a>(&'a self, hash: &'a [u8]) -> Fut<'a, Option<SessionRecord>>;
    fn touch_session<'a>(&'a self, hash: &'a [u8], now: Timestamp) -> Fut<'a, ()>;
    fn delete_session<'a>(&'a self, hash: &'a [u8]) -> Fut<'a, ()>;

    fn create_token<'a>(&'a self, new: NewTokenRecord<'a>) -> Fut<'a, TokenInfo>;
    fn list_tokens(&self, user_id: i64) -> Fut<'_, Vec<TokenInfo>>;
    /// Whether a token of `user_id`'s was deleted.
    fn delete_token(&self, user_id: i64, token_id: i64) -> Fut<'_, bool>;
    /// A token that hasn't expired at `now`, of an account that isn't suspended.
    fn find_live_token<'a>(&'a self, hash: &'a [u8], now: Timestamp) -> Fut<'a, Option<TokenRecord>>;
    fn touch_token(&self, token_id: i64, now: Timestamp) -> Fut<'_, ()>;

    fn find_repo<'a>(&'a self, owner_key: &'a str, repo_key: &'a str) -> Fut<'a, Option<Repo>>;
    /// One page of an owner's repositories, ordered by name key, after `after_key`.
    fn list_repos<'a>(&'a self, page: RepoPage<'a>) -> Fut<'a, Vec<Repo>>;
    /// Every repository, by ID.
    fn all_repos(&self) -> Fut<'_, Vec<Repo>>;
    fn set_private(&self, repo_id: RepoId, private: bool) -> Fut<'_, ()>;
}

/// Writes inside one transaction. It holds the write lock from the start, so a
/// read followed by a write can't be overtaken by another writer.
pub(crate) trait WriteTx: Send {
    /// Uses up an unused, unexpired invite; `false` if there is none.
    fn redeem_invite<'a>(&'a mut self, hash: &'a [u8], now: Timestamp) -> Fut<'a, bool>;
    /// Fails with `OwnerExists` or `EmailExists` on a taken name or address.
    fn create_user<'a>(&'a mut self, new: NewUserRecord<'a>) -> Fut<'a, User>;
    /// Fails with `RepoExists` on a taken name.
    fn create_repo<'a>(&'a mut self, new: NewRepoRecord<'a>) -> Fut<'a, Repo>;
    fn commit(self: Box<Self>) -> Fut<'static, ()>;
}

/// A user, with what signing in needs.
pub(crate) struct UserRecord {
    pub user: User,
    pub password_hash: Option<String>,
}

pub(crate) struct SessionRecord {
    pub user: User,
    pub created_at: Timestamp,
    pub last_seen_at: Timestamp,
}

pub(crate) struct TokenRecord {
    pub id: i64,
    pub scopes: Scopes,
    pub last_used_at: Option<Timestamp>,
    pub user: User,
}

pub(crate) struct NewUserRecord<'a> {
    pub name: &'a str,
    pub name_key: &'a str,
    /// The address as typed, and its lowercased key. It becomes the primary one.
    pub email: Option<(&'a str, &'a str)>,
    pub password_hash: Option<&'a str>,
    pub admin: bool,
    pub now: Timestamp,
}

pub(crate) struct NewTokenRecord<'a> {
    pub user_id: i64,
    pub name: &'a str,
    pub hash: &'a [u8],
    pub scopes: &'a Scopes,
    pub now: Timestamp,
    pub expires_at: Option<Timestamp>,
}

pub(crate) struct NewRepoRecord<'a> {
    pub owner: &'a Owner,
    pub name: &'a str,
    pub name_key: &'a str,
    pub private: bool,
    pub now: Timestamp,
}

pub(crate) struct RepoPage<'a> {
    pub owner_id: i64,
    pub after_key: &'a str,
    pub include_private: bool,
    pub limit: u32,
}
