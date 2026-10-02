pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("repository {0} not found")]
    RepoNotFound(String),
    #[error("repository {0} already exists")]
    RepoExists(String),
    #[error("{0:?} is not a bare repository under the storage root")]
    NotARepository(String),
    #[error("revision {0:?} not found")]
    RevisionNotFound(String),
    #[error("invalid cursor {0:?}")]
    InvalidCursor(String),
    #[error("path {0:?} not found")]
    PathNotFound(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Git(Box<dyn std::error::Error + Send + Sync>),
}

impl Error {
    /// Wraps any gix error. gix has a distinct error type per operation, so they
    /// are boxed rather than given a variant each.
    pub(crate) fn git(err: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Git(Box::new(err))
    }
}
