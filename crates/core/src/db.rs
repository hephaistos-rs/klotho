//! The SQLite metadata store (FR-STOR-001, 002).

use std::path::Path;
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

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
        .foreign_keys(true)
        // Writes are serialised; wait for the lock instead of failing at once.
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new().max_connections(8).connect_with(options).await?;
    MIGRATOR.run(&pool).await?;
    Ok(pool)
}

/// Now, as RFC 3339 text in UTC: how timestamps are stored.
pub(crate) fn now() -> String {
    jiff::Timestamp::now().to_string()
}

/// Reads a timestamp written by [`now`].
pub(crate) fn parse_time(text: &str) -> jiff::Timestamp {
    text.parse().unwrap_or_else(|err| {
        tracing::error!(%err, text, "unreadable timestamp in the database");
        jiff::Timestamp::UNIX_EPOCH
    })
}
