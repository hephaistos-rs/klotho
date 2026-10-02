//! Klotho's own implementation of the server side of the git protocol, so no
//! `git` program is needed (ADR 0004). It has no HTTP or SSH in it: callers
//! hand it a repository and the request bytes, and it writes the response.
//!
//! Built so far (Phase 1b, step 1): protocol v2 `ls-refs` and `fetch`, without
//! shallow clones or filters. Pushes and protocol v0/v1 still go to the `git`
//! program in `klotho-server`.

mod pack;
pub mod pktline;
mod sideband;
mod v2;

pub use v2::{needs_git_program, serve as serve_v2, write_advertisement as write_v2_advertisement};

/// Something wrong with what the client sent. Its message goes back to the
/// client, so it must never contain server details.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ProtocolError(String);

impl ProtocolError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    /// Writing the response failed, usually because the client went away.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Git(#[from] crate::Error),
}
