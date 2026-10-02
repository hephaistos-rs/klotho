//! Repository storage, read access and the git wire protocol for Klotho, all
//! in-process with gitoxide. Klotho never runs the `git` program (ADR 0004).
//!
//! This crate only touches repositories on disk, addressed by ID. It knows
//! nothing about names, owners or users; those live in `klotho-core`.

mod browse;
mod error;
pub mod protocol;
mod store;

pub use browse::{CommitInfo, RefInfo, RepoInfo, TreeEntryInfo, read_blob};
pub use error::{Error, Result};
pub use store::{RepoId, RepoStore};
