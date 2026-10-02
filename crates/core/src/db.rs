//! The SQLite metadata store (FR-STOR-001, 002). The schema's conventions are
//! at the top of the first migration.

use std::path::Path;
use std::time::Duration;

use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::Result;

/// The migrations in `crates/core/migrations`, embedded in the binary. They run
/// forward-only, each in a transaction, at startup. A database with a migration
/// this binary doesn't know (a newer schema) refuses to start (NFR-OPS-031).
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Opens (creating if needed) the database at `path` and brings its schema up to date.
pub async fn open(path: &Path) -> Result<SqlitePool> {
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
    Ok(pool)
}

/// Starts a transaction that will write. It takes the write lock at once
/// (`BEGIN IMMEDIATE`): a deferred transaction that reads first and then writes
/// fails with `SQLITE_BUSY` if another writer got in between, without waiting
/// for the busy timeout.
pub(crate) async fn begin_write(pool: &SqlitePool) -> Result<Transaction<'static, Sqlite>> {
    Ok(pool.begin_with("BEGIN IMMEDIATE").await?)
}

/// Now, as stored: seconds since the Unix epoch.
pub(crate) fn now() -> i64 {
    jiff::Timestamp::now().as_second()
}

/// A stored time, as a timestamp.
pub(crate) fn time(seconds: i64) -> jiff::Timestamp {
    jiff::Timestamp::from_second(seconds).unwrap_or_else(|err| {
        tracing::error!(%err, seconds, "time out of range in the database");
        jiff::Timestamp::UNIX_EPOCH
    })
}

#[cfg(test)]
mod tests {
    use crate::NewUser;
    use crate::testing::core;

    /// The schema enforces its rules itself, whatever the code above it does.
    #[tokio::test]
    async fn the_schema_rejects_what_breaks_its_rules() {
        let (_dir, core) = core().await;
        let alice = NewUser { email: Some("alice@example.com"), ..NewUser::named("alice") };
        let alice = core.create_user(alice).await.unwrap();
        core.create_repo("alice", "demo", false).await.unwrap();
        let db = &core.db;

        // STRICT: text where a time belongs.
        let wrong_type = sqlx::query!("UPDATE owners SET created_at = 'yesterday' WHERE id = ?", alice.id);
        assert!(wrong_type.execute(db).await.is_err());
        // Booleans are 0 or 1.
        assert!(
            sqlx::query!("UPDATE users SET is_admin = 2 WHERE id = ?", alice.id).execute(db).await.is_err()
        );
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
