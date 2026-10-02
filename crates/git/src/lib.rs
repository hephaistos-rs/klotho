//! Repository storage, read access and the git wire protocol for Klotho, all
//! in-process with gitoxide. Klotho never runs the `git` program (ADR 0004).
//!
//! This crate only touches repositories on disk, addressed by ID. It knows
//! nothing about names, owners or users; those live in `klotho-core`.

mod blob;
mod browse;
mod contents;
mod error;
#[cfg(test)]
mod fixture;
pub mod protocol;
mod store;

pub use blob::BlobReader;
pub use browse::{
    Annotation, BranchInfo, CommitInfo, GitTime, Page, RefKind, RefTarget, RepoInfo, Resolved, Signature,
    TagInfo, branch, branches, log, resolve, resolve_ref, tag, tags,
};
pub use contents::{
    Contents, DirListing, EntryType, FileInfo, LinkInfo, SubmoduleInfo, TEXT_LIMIT, TreeEntry, contents,
    open_file, readme,
};
pub use error::{Error, Result};
pub use store::{RepoId, RepoStore};
