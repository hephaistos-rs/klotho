//! Repository storage and read access for Klotho, backed by gix.
//!
//! This crate only touches repositories on disk, addressed by ID. It knows
//! nothing about names, owners or users; those live in `klotho-core`. Serving
//! the git wire protocol (clone, fetch, push) is still done by the server with
//! the `git` program, until the native engine replaces it (ADR 0004).

mod browse;
mod error;
mod store;
mod version;

pub use browse::{CommitInfo, RefInfo, RepoInfo, TreeEntryInfo, read_blob};
pub use error::{Error, Result};
pub use store::{RepoId, RepoStore};
pub use version::{GitCheckError, GitVersion, check_git};
