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
    /// Wrong username or password, or an account that can't sign in. Never says which.
    #[error("incorrect username or password")]
    InvalidCredentials,
    /// The actor may see the resource but not do this to it. Without read
    /// access the answer is `RepoNotFound` instead, so private names can't be
    /// discovered (FR-ACL-013).
    #[error("you don't have permission to do that")]
    Forbidden,
    #[error("registration is closed")]
    RegistrationClosed,
    #[error("the invite link is invalid, used or expired")]
    InvalidInvite,
    #[error("that email address is already in use")]
    EmailExists,
    #[error("token not found")]
    TokenNotFound,
    /// Input that breaks a rule; the message says which, for the user.
    #[error("{0}")]
    InvalidInput(String),
    #[error("{0}")]
    Internal(String),
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
