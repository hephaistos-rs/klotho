use crate::names::InvalidName;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Domain errors. `klotho-server` and `klotho-web` turn them into status codes
/// and pages; nothing here knows about HTTP.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    InvalidName(#[from] InvalidName),
    #[error("owner {0} not found")]
    OwnerNotFound(String),
    #[error("repository {0} not found")]
    RepoNotFound(String),
    #[error("the name {0} is already taken")]
    OwnerExists(String),
    #[error("repository {0} already exists")]
    RepoExists(String),
    #[error("{0} is not an unadopted repository")]
    NotUnadopted(String),
    #[error("repository storage {0} is not available")]
    StorageUnavailable(String),
    #[error(transparent)]
    Git(#[from] klotho_git::Error),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("background task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

pub(crate) fn is_unique_violation(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::Database(db) if db.is_unique_violation())
}
