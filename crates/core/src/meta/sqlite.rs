//! The SQLite metadata store, the default (FR-STOR-002). The schema and its
//! conventions are in `migrations/sqlite/`. Times are stored as INTEGER seconds
//! since the Unix epoch.

use std::path::Path;
use std::time::Duration;

use jiff::Timestamp;
use klotho_git::RepoId;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::{
    Fut, MetaStore, NewRepoRecord, NewTokenRecord, NewUserRecord, RepoPage, SessionRecord, TokenRecord,
    UserRecord, WriteTx,
};
use crate::{Error, Owner, OwnerKind, Repo, Result, Scopes, TokenInfo, User};

/// The migrations, embedded in the binary. They run forward-only, each in a
/// transaction, at startup. A database with a migration this binary doesn't
/// know (a newer schema) refuses to start (NFR-OPS-031).
static MIGRATOR: Migrator = sqlx::migrate!("./migrations/sqlite");

pub(crate) struct SqliteMeta {
    pool: SqlitePool,
}

impl SqliteMeta {
    /// Opens (creating if needed) the database at `path` and brings its schema up to date.
    pub(crate) async fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(sqlx::Error::Io)?;
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            // Readers don't block the writer, and a crash can't corrupt the file.
            .journal_mode(SqliteJournalMode::Wal)
            // With WAL, NORMAL is still crash-safe; only the last commits before a
            // power loss can be lost. FULL would sync on every commit.
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            // Writes are serialised; wait for the lock instead of failing at once.
            .busy_timeout(Duration::from_secs(5))
            // Keeps the query planner's statistics current, cheaply.
            .optimize_on_close(true, 400);
        let pool = SqlitePoolOptions::new().max_connections(8).connect_with(options).await?;
        MIGRATOR.run(&pool).await?;
        Ok(Self { pool })
    }

    /// For tests that set up states the services can't, such as old sessions.
    #[cfg(test)]
    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

fn secs(time: Timestamp) -> i64 {
    time.as_second()
}

fn time(seconds: i64) -> Timestamp {
    Timestamp::from_second(seconds).unwrap_or_else(|err| {
        tracing::error!(%err, seconds, "time out of range in the database");
        Timestamp::UNIX_EPOCH
    })
}

fn owner_kind(kind: &str) -> OwnerKind {
    if kind == "org" { OwnerKind::Org } else { OwnerKind::User }
}

fn scopes(text: &str) -> Scopes {
    text.parse().unwrap_or_else(|err| {
        tracing::error!(%err, text, "unreadable token scopes in the database");
        Scopes::default()
    })
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::Database(db) if db.is_unique_violation())
}

/// A row with the repository columns every repository query selects.
macro_rules! repo_from_row {
    ($row:expr) => {
        Repo {
            id: $row.id,
            owner: Owner { id: $row.owner_id, name: $row.owner_name, kind: owner_kind(&$row.owner_kind) },
            name: $row.name,
            private: $row.private,
            created_at: time($row.created_at),
        }
    };
}

impl MetaStore for SqliteMeta {
    fn ping(&self) -> Fut<'_, ()> {
        Box::pin(async move {
            sqlx::query!("SELECT 1 AS one").fetch_one(&self.pool).await?;
            Ok(())
        })
    }

    fn begin_write(&self) -> Fut<'_, Box<dyn WriteTx>> {
        Box::pin(async move {
            // BEGIN IMMEDIATE takes the write lock at once. A deferred transaction
            // that reads and then writes fails with SQLITE_BUSY if another writer
            // got in between, without waiting for the busy timeout.
            let tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
            Ok(Box::new(SqliteWriteTx { tx }) as Box<dyn WriteTx>)
        })
    }

    fn find_owner<'a>(&'a self, name_key: &'a str) -> Fut<'a, Option<Owner>> {
        Box::pin(async move {
            let row =
                sqlx::query!(r#"SELECT id AS "id!", name, kind FROM owners WHERE name_key = ?"#, name_key)
                    .fetch_optional(&self.pool)
                    .await?;
            Ok(row.map(|row| Owner { id: row.id, name: row.name, kind: owner_kind(&row.kind) }))
        })
    }

    fn find_user<'a>(&'a self, name_key: &'a str) -> Fut<'a, Option<UserRecord>> {
        Box::pin(async move {
            let row = sqlx::query!(
                r#"SELECT u.id AS "id!", o.name, u.is_admin AS "is_admin: bool", u.suspended_at, u.password_hash
                   FROM users u JOIN owners o ON o.id = u.id WHERE o.name_key = ?"#,
                name_key
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|row| UserRecord {
                user: User {
                    id: row.id,
                    username: row.name,
                    is_admin: row.is_admin,
                    suspended: row.suspended_at.is_some(),
                },
                password_hash: row.password_hash,
            }))
        })
    }

    fn create_invite<'a>(&'a self, hash: &'a [u8], now: Timestamp, expires_at: Timestamp) -> Fut<'a, ()> {
        Box::pin(async move {
            let (now, expires) = (secs(now), secs(expires_at));
            sqlx::query!(
                "INSERT INTO one_time_tokens (token_hash, purpose, created_at, expires_at) VALUES (?, 'invite', ?, ?)",
                hash,
                now,
                expires
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
    }

    fn create_session<'a>(&'a self, hash: &'a [u8], user_id: i64, now: Timestamp) -> Fut<'a, ()> {
        Box::pin(async move {
            let now = secs(now);
            sqlx::query!(
                "INSERT INTO sessions (id_hash, user_id, created_at, last_seen_at) VALUES (?, ?, ?, ?)",
                hash,
                user_id,
                now,
                now
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
    }

    fn find_session<'a>(&'a self, hash: &'a [u8]) -> Fut<'a, Option<SessionRecord>> {
        Box::pin(async move {
            let row = sqlx::query!(
                r#"SELECT s.created_at, s.last_seen_at, u.id AS "id!", o.name, u.is_admin AS "is_admin: bool", u.suspended_at
                   FROM sessions s JOIN users u ON u.id = s.user_id JOIN owners o ON o.id = u.id
                   WHERE s.id_hash = ?"#,
                hash
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|row| SessionRecord {
                user: User {
                    id: row.id,
                    username: row.name,
                    is_admin: row.is_admin,
                    suspended: row.suspended_at.is_some(),
                },
                created_at: time(row.created_at),
                last_seen_at: time(row.last_seen_at),
            }))
        })
    }

    fn touch_session<'a>(&'a self, hash: &'a [u8], now: Timestamp) -> Fut<'a, ()> {
        Box::pin(async move {
            let now = secs(now);
            sqlx::query!("UPDATE sessions SET last_seen_at = ? WHERE id_hash = ?", now, hash)
                .execute(&self.pool)
                .await?;
            Ok(())
        })
    }

    fn delete_session<'a>(&'a self, hash: &'a [u8]) -> Fut<'a, ()> {
        Box::pin(async move {
            sqlx::query!("DELETE FROM sessions WHERE id_hash = ?", hash).execute(&self.pool).await?;
            Ok(())
        })
    }

    fn create_token<'a>(&'a self, new: NewTokenRecord<'a>) -> Fut<'a, TokenInfo> {
        Box::pin(async move {
            let (scope_text, now, expires) =
                (new.scopes.to_string(), secs(new.now), new.expires_at.map(secs));
            let id = sqlx::query_scalar!(
                r#"INSERT INTO access_tokens (user_id, name, token_hash, scopes, created_at, expires_at)
                   VALUES (?, ?, ?, ?, ?, ?) RETURNING id AS "id!""#,
                new.user_id,
                new.name,
                new.hash,
                scope_text,
                now,
                expires
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(TokenInfo {
                id,
                name: new.name.to_owned(),
                scopes: new.scopes.clone(),
                created_at: time(now),
                expires_at: expires.map(time),
                last_used_at: None,
            })
        })
    }

    fn list_tokens(&self, user_id: i64) -> Fut<'_, Vec<TokenInfo>> {
        Box::pin(async move {
            let rows = sqlx::query!(
                r#"SELECT id AS "id!", name, scopes, created_at, expires_at, last_used_at
                   FROM access_tokens WHERE user_id = ? ORDER BY id"#,
                user_id
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|row| TokenInfo {
                    id: row.id,
                    name: row.name,
                    scopes: scopes(&row.scopes),
                    created_at: time(row.created_at),
                    expires_at: row.expires_at.map(time),
                    last_used_at: row.last_used_at.map(time),
                })
                .collect())
        })
    }

    fn delete_token(&self, user_id: i64, token_id: i64) -> Fut<'_, bool> {
        Box::pin(async move {
            let deleted =
                sqlx::query!("DELETE FROM access_tokens WHERE id = ? AND user_id = ?", token_id, user_id)
                    .execute(&self.pool)
                    .await?;
            Ok(deleted.rows_affected() > 0)
        })
    }

    fn find_live_token<'a>(&'a self, hash: &'a [u8], now: Timestamp) -> Fut<'a, Option<TokenRecord>> {
        Box::pin(async move {
            let now = secs(now);
            let row = sqlx::query!(
                r#"SELECT t.id AS "token_id!", t.scopes, t.last_used_at,
                          u.id AS "id!", o.name, u.is_admin AS "is_admin: bool"
                   FROM access_tokens t JOIN users u ON u.id = t.user_id JOIN owners o ON o.id = u.id
                   WHERE t.token_hash = ?1 AND (t.expires_at IS NULL OR t.expires_at > ?2)
                     AND u.suspended_at IS NULL"#,
                hash,
                now
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|row| TokenRecord {
                id: row.token_id,
                scopes: scopes(&row.scopes),
                last_used_at: row.last_used_at.map(time),
                user: User { id: row.id, username: row.name, is_admin: row.is_admin, suspended: false },
            }))
        })
    }

    fn touch_token(&self, token_id: i64, now: Timestamp) -> Fut<'_, ()> {
        Box::pin(async move {
            let now = secs(now);
            sqlx::query!("UPDATE access_tokens SET last_used_at = ? WHERE id = ?", now, token_id)
                .execute(&self.pool)
                .await?;
            Ok(())
        })
    }

    fn find_repo<'a>(&'a self, owner_key: &'a str, repo_key: &'a str) -> Fut<'a, Option<Repo>> {
        Box::pin(async move {
            let row = sqlx::query!(
                r#"SELECT r.id AS "id!", r.name, r.created_at, r.private AS "private: bool",
                          o.id AS "owner_id!", o.name AS owner_name, o.kind AS owner_kind
                   FROM repositories r JOIN owners o ON o.id = r.owner_id
                   WHERE o.name_key = ? AND r.name_key = ?"#,
                owner_key,
                repo_key,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|row| repo_from_row!(row)))
        })
    }

    fn list_repos<'a>(&'a self, page: RepoPage<'a>) -> Fut<'a, Vec<Repo>> {
        Box::pin(async move {
            let limit = i64::from(page.limit);
            let rows = sqlx::query!(
                r#"SELECT r.id AS "id!", r.name, r.created_at, r.private AS "private: bool",
                          o.id AS "owner_id!", o.name AS owner_name, o.kind AS owner_kind
                   FROM repositories r JOIN owners o ON o.id = r.owner_id
                   WHERE r.owner_id = ? AND r.name_key > ? AND (r.private = 0 OR ?)
                   ORDER BY r.name_key LIMIT ?"#,
                page.owner_id,
                page.after_key,
                page.include_private,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows.into_iter().map(|row| repo_from_row!(row)).collect())
        })
    }

    fn all_repos(&self) -> Fut<'_, Vec<Repo>> {
        Box::pin(async move {
            let rows = sqlx::query!(
                r#"SELECT r.id AS "id!", r.name, r.created_at, r.private AS "private: bool",
                          o.id AS "owner_id!", o.name AS owner_name, o.kind AS owner_kind
                   FROM repositories r JOIN owners o ON o.id = r.owner_id ORDER BY r.id"#
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows.into_iter().map(|row| repo_from_row!(row)).collect())
        })
    }

    fn set_private(&self, repo_id: RepoId, private: bool) -> Fut<'_, ()> {
        Box::pin(async move {
            sqlx::query!("UPDATE repositories SET private = ? WHERE id = ?", private, repo_id)
                .execute(&self.pool)
                .await?;
            Ok(())
        })
    }
}

struct SqliteWriteTx {
    tx: Transaction<'static, Sqlite>,
}

impl WriteTx for SqliteWriteTx {
    fn redeem_invite<'a>(&'a mut self, hash: &'a [u8], now: Timestamp) -> Fut<'a, bool> {
        Box::pin(async move {
            // Checked and used up in one statement, so two sign-ups can't share one invite.
            let now = secs(now);
            let used = sqlx::query!(
                "UPDATE one_time_tokens SET used_at = ?1
                 WHERE token_hash = ?2 AND purpose = 'invite' AND used_at IS NULL AND expires_at > ?1",
                now,
                hash
            )
            .execute(&mut *self.tx)
            .await?;
            Ok(used.rows_affected() == 1)
        })
    }

    fn create_user<'a>(&'a mut self, new: NewUserRecord<'a>) -> Fut<'a, User> {
        Box::pin(async move {
            let now = secs(new.now);
            let id = sqlx::query_scalar!(
                r#"INSERT INTO owners (kind, name, name_key, created_at) VALUES ('user', ?, ?, ?) RETURNING id AS "id!""#,
                new.name,
                new.name_key,
                now,
            )
            .fetch_one(&mut *self.tx)
            .await
            .map_err(|err| if is_unique_violation(&err) { Error::OwnerExists(new.name.to_owned()) } else { err.into() })?;
            sqlx::query!(
                "INSERT INTO users (id, password_hash, is_admin) VALUES (?, ?, ?)",
                id,
                new.password_hash,
                new.admin
            )
            .execute(&mut *self.tx)
            .await?;
            if let Some((address, key)) = new.email {
                sqlx::query!(
                    "INSERT INTO emails (user_id, address, address_key, is_primary) VALUES (?, ?, ?, 1)",
                    id,
                    address,
                    key
                )
                .execute(&mut *self.tx)
                .await
                .map_err(|err| if is_unique_violation(&err) { Error::EmailExists } else { err.into() })?;
            }
            Ok(User { id, username: new.name.to_owned(), is_admin: new.admin, suspended: false })
        })
    }

    fn create_repo<'a>(&'a mut self, new: NewRepoRecord<'a>) -> Fut<'a, Repo> {
        Box::pin(async move {
            let now = secs(new.now);
            let id = sqlx::query_scalar!(
                r#"INSERT INTO repositories (owner_id, name, name_key, created_at, private)
                   VALUES (?, ?, ?, ?, ?) RETURNING id AS "id!""#,
                new.owner.id,
                new.name,
                new.name_key,
                now,
                new.private,
            )
            .fetch_one(&mut *self.tx)
            .await
            .map_err(|err| {
                if is_unique_violation(&err) {
                    Error::RepoExists(format!("{}/{}", new.owner.name, new.name))
                } else {
                    err.into()
                }
            })?;
            Ok(Repo {
                id,
                owner: new.owner.clone(),
                name: new.name.to_owned(),
                private: new.private,
                created_at: time(now),
            })
        })
    }

    fn commit(self: Box<Self>) -> Fut<'static, ()> {
        Box::pin(async move { Ok(self.tx.commit().await?) })
    }
}

#[cfg(test)]
mod tests {
    use crate::NewUser;
    use crate::testing::core_and_pool;

    /// The schema enforces its rules itself, whatever the code above it does.
    #[tokio::test]
    async fn the_schema_rejects_what_breaks_its_rules() {
        let (_dir, core, db) = core_and_pool().await;
        let alice = NewUser { email: Some("alice@example.com"), ..NewUser::named("alice") };
        let alice = core.create_user(alice).await.unwrap();
        core.create_repo("alice", "demo", false).await.unwrap();
        let db = &db;

        // STRICT: text where a time belongs.
        let wrong_type = sqlx::query!("UPDATE owners SET created_at = 'yesterday' WHERE id = ?", alice.id);
        assert!(wrong_type.execute(db).await.is_err());
        // Booleans are 0 or 1.
        let not_bool = sqlx::query!("UPDATE users SET is_admin = 2 WHERE id = ?", alice.id);
        assert!(not_bool.execute(db).await.is_err());
        // One primary address per user.
        let second_primary = sqlx::query!(
            "INSERT INTO emails (user_id, address, address_key, is_primary) VALUES (?, 'a2@example.com', 'a2@example.com', 1)",
            alice.id
        );
        assert!(second_primary.execute(db).await.is_err());
        // Hashes are 32 bytes.
        let short: &[u8] = b"short";
        let session = sqlx::query!(
            "INSERT INTO sessions (id_hash, user_id, created_at, last_seen_at) VALUES (?, ?, 0, 0)",
            short,
            alice.id
        );
        assert!(session.execute(db).await.is_err());
        // An owner with repositories can't be deleted.
        assert!(sqlx::query!("DELETE FROM owners WHERE id = ?", alice.id).execute(db).await.is_err());
    }
}
