//! Repository storage, read access and the git wire protocol for Klotho, all
//! in-process with gitoxide. Klotho never runs the `git` program (ADR 0004).
//!
//! This crate only touches repositories on disk, addressed by ID. It knows
//! nothing about names, owners or users; those live in `klotho-core`.

mod browse;
mod error;
pub mod protocol;
mod store;

pub use browse::{
    Annotation, BranchInfo, CommitInfo, GitTime, Page, RefKind, RefTarget, RepoInfo, Resolved, Signature,
    TagInfo, TreeEntryInfo, branch, branches, log, read_blob, resolve, resolve_ref, tag, tags,
};
pub use error::{Error, Result};
pub use store::{RepoId, RepoStore};
