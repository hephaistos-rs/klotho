//! Repository storage and read access for Klotho, backed by gix.
//!
//! This crate only touches repositories on disk. Serving the git wire protocol
//! (clone, fetch, push) is done by the server, which delegates to the `git` binary
//! because gix does not implement the server side of the protocol.

mod browse;
mod error;
mod name;
mod store;
mod version;

pub use browse::{CommitInfo, RefInfo, RepoInfo, TreeEntryInfo, read_blob};
pub use error::{Error, Result};
pub use name::RepoName;
pub use store::RepoStore;
pub use version::{GitCheckError, GitVersion, check_git};
