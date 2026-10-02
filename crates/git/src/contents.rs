//! What is at a path in a commit: a directory listing, a file's metadata and,
//! for a small text file, its text (FR-UI-001, FR-UI-003).

use std::io::Read;

use gix::ObjectId;
use gix::objs::tree::EntryKind;
use serde::Serialize;

use crate::blob::BlobReader;
use crate::browse::{RefTarget, commit_for};
use crate::{Error, Result};

/// Files up to this size come with their text; larger ones only with metadata.
pub const TEXT_LIMIT: u64 = 1024 * 1024;
/// How much of a file is checked for a NUL byte to call it binary, as git does.
const SNIFF: usize = 8000;

/// What a path in a commit is.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Contents {
    Dir(DirListing),
    File(FileInfo),
    Symlink(LinkInfo),
    Submodule(SubmoduleInfo),
}

#[derive(Debug, Serialize)]
pub struct DirListing {
    /// Empty for the root.
    pub path: String,
    /// Directories first, then files, each by name.
    pub entries: Vec<TreeEntry>,
    /// The cursor for the next page of entries, if there is one.
    pub next: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryType {
    Dir,
    File,
    Executable,
    Symlink,
    Submodule,
}

#[derive(Debug, Serialize)]
pub struct TreeEntry {
    pub name: String,
    pub path: String,
    #[serde(rename = "type")]
    pub kind: EntryType,
    /// The tree, blob or (for a submodule) commit ID.
    pub id: String,
    /// For files and symlinks.
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub id: String,
    pub executable: bool,
    pub size: u64,
    /// A NUL byte in the first 8000 bytes, or text that isn't UTF-8.
    pub binary: bool,
    /// The text, for a text file up to [`TEXT_LIMIT`].
    pub text: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LinkInfo {
    pub name: String,
    pub path: String,
    pub id: String,
    pub target: String,
}

#[derive(Debug, Serialize)]
pub struct SubmoduleInfo {
    pub name: String,
    pub path: String,
    pub commit: String,
}

/// What is at `path` in `rev`. Directory entries come `limit` at a time; the
/// cursor is from the previous page's `next`.
pub fn contents(
    repo: &gix::Repository,
    rev: Option<&str>,
    path: &str,
    cursor: Option<&str>,
    limit: usize,
) -> Result<Contents> {
    let path = path.trim_matches('/');
    let root = commit_for(repo, rev)?.1.tree().map_err(Error::git)?;
    if path.is_empty() {
        return Ok(Contents::Dir(list(repo, &root, path, cursor, limit)?));
    }
    let entry = root.lookup_entry_by_path(path).map_err(Error::git)?;
    let entry = entry.ok_or_else(|| Error::PathNotFound(path.to_owned()))?;
    let (id, name) = (entry.object_id(), entry.filename().to_string());
    Ok(match entry.mode().kind() {
        EntryKind::Tree => {
            let tree = repo.find_tree(id).map_err(Error::git)?;
            Contents::Dir(list(repo, &tree, path, cursor, limit)?)
        }
        EntryKind::Blob | EntryKind::BlobExecutable => {
            let executable = entry.mode().kind() == EntryKind::BlobExecutable;
            Contents::File(file_info(repo, id, name, path.to_owned(), executable)?)
        }
        EntryKind::Link => {
            let target = repo.find_blob(id).map_err(Error::git)?.data.clone();
            let target = String::from_utf8_lossy(&target).into_owned();
            Contents::Symlink(LinkInfo { name, path: path.to_owned(), id: id.to_string(), target })
        }
        EntryKind::Commit => {
            Contents::Submodule(SubmoduleInfo { name, path: path.to_owned(), commit: id.to_string() })
        }
    })
}

/// The README in the directory at `dir` (the root if empty), if there is one:
/// `README.md` first, then other Markdown, plain `README`, then anything else
/// called `README.*`, ignoring case.
pub fn readme(repo: &gix::Repository, rev: Option<&str>, dir: &str) -> Result<Option<FileInfo>> {
    let dir = dir.trim_matches('/');
    let root = commit_for(repo, rev)?.1.tree().map_err(Error::git)?;
    let tree = if dir.is_empty() {
        root
    } else {
        let entry = root.lookup_entry_by_path(dir).map_err(Error::git)?;
        let entry = entry.filter(|entry| entry.mode().is_tree());
        let entry = entry.ok_or_else(|| Error::PathNotFound(dir.to_owned()))?;
        repo.find_tree(entry.object_id()).map_err(Error::git)?
    };

    let mut best: Option<(u8, String, ObjectId, bool)> = None;
    for entry in tree.iter() {
        let entry = entry.map_err(Error::git)?;
        let kind = entry.mode().kind();
        if !matches!(kind, EntryKind::Blob | EntryKind::BlobExecutable) {
            continue;
        }
        let name = entry.filename().to_string();
        let Some(rank) = readme_rank(&name) else { continue };
        if best.as_ref().is_none_or(|(r, n, ..)| (rank, &name) < (*r, n)) {
            best = Some((rank, name, entry.oid().to_owned(), kind == EntryKind::BlobExecutable));
        }
    }
    let Some((_, name, id, executable)) = best else { return Ok(None) };
    let path = if dir.is_empty() { name.clone() } else { format!("{dir}/{name}") };
    Ok(Some(file_info(repo, id, name, path, executable)?))
}

fn readme_rank(name: &str) -> Option<u8> {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "readme.md" => Some(0),
        "readme.markdown" | "readme.mdown" | "readme.mkd" => Some(1),
        "readme" => Some(2),
        "readme.txt" => Some(3),
        _ if lower.starts_with("readme.") => Some(4),
        _ => None,
    }
}

/// The file at `path` in `rev`, opened for reading. Symlinks give their target.
pub fn open_file(repo: &gix::Repository, rev: Option<&str>, path: &str) -> Result<(RefTarget, BlobReader)> {
    let path = path.trim_matches('/');
    let (target, commit) = commit_for(repo, rev)?;
    let root = commit.tree().map_err(Error::git)?;
    let entry = root.lookup_entry_by_path(path).map_err(Error::git)?;
    let entry = entry.filter(|entry| entry.mode().is_blob_or_symlink());
    let entry = entry.ok_or_else(|| Error::PathNotFound(path.to_owned()))?;
    Ok((target, BlobReader::open(repo, entry.object_id())?))
}

fn list(
    repo: &gix::Repository,
    tree: &gix::Tree<'_>,
    path: &str,
    cursor: Option<&str>,
    limit: usize,
) -> Result<DirListing> {
    let mut entries = Vec::new();
    for entry in tree.iter() {
        let entry = entry.map_err(Error::git)?;
        let kind = match entry.mode().kind() {
            EntryKind::Tree => EntryType::Dir,
            EntryKind::Blob => EntryType::File,
            EntryKind::BlobExecutable => EntryType::Executable,
            EntryKind::Link => EntryType::Symlink,
            EntryKind::Commit => EntryType::Submodule,
        };
        let name = entry.filename().to_string();
        let key = sort_key(kind, &name);
        if cursor.is_some_and(|cursor| key.as_str() <= cursor) {
            continue;
        }
        entries.push((key, name, kind, entry.oid().to_owned()));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let next = (entries.len() > limit).then(|| entries[limit - 1].0.clone());
    entries.truncate(limit);

    let entries = entries
        .into_iter()
        .map(|(_, name, kind, id)| {
            let size = match kind {
                EntryType::Dir | EntryType::Submodule => None,
                _ => Some(repo.find_header(id).map_err(Error::git)?.size()),
            };
            let path = if path.is_empty() { name.clone() } else { format!("{path}/{name}") };
            Ok(TreeEntry { name, path, kind, id: id.to_string(), size })
        })
        .collect::<Result<_>>()?;
    Ok(DirListing { path: path.to_owned(), entries, next })
}

/// Directories sort before everything else, then by name. The key doubles as
/// the cursor.
fn sort_key(kind: EntryType, name: &str) -> String {
    let group = if kind == EntryType::Dir { '0' } else { '1' };
    format!("{group}{name}")
}

fn file_info(
    repo: &gix::Repository,
    id: ObjectId,
    name: String,
    path: String,
    executable: bool,
) -> Result<FileInfo> {
    let reader = BlobReader::open(repo, id)?;
    let size = reader.size();
    let wanted = if size <= TEXT_LIMIT { size } else { SNIFF as u64 };
    let mut head = Vec::new();
    reader.take(wanted).read_to_end(&mut head)?;
    let mut binary = head[..head.len().min(SNIFF)].contains(&0);
    let text = if binary || size > TEXT_LIMIT {
        None
    } else {
        match String::from_utf8(head) {
            Ok(text) => Some(text),
            Err(_) => {
                binary = true;
                None
            }
        }
    };
    Ok(FileInfo { name, path, id: id.to_string(), executable, size, binary, text })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::Fixture;

    fn fixture() -> Fixture {
        let f = Fixture::new();
        let submodule = "0123456789abcdef0123456789abcdef01234567".as_bytes();
        let entries: &[(&str, EntryKind, &[u8])] = &[
            ("README.md", EntryKind::Blob, b"# Hello\n"),
            ("readme.txt", EntryKind::Blob, b"plain\n"),
            ("src/main.rs", EntryKind::Blob, b"fn main() {}\n"),
            ("src/deep/x", EntryKind::Blob, b"x"),
            ("run.sh", EntryKind::BlobExecutable, b"#!/bin/sh\n"),
            ("link", EntryKind::Link, b"src/main.rs"),
            ("logo.png", EntryKind::Blob, b"\x89PNG\r\n\x1a\n\0\0\0"),
            ("latin1.txt", EntryKind::Blob, b"caf\xe9\n"),
            ("vendor", EntryKind::Commit, submodule),
            ("a-dir/f", EntryKind::Blob, b""),
        ];
        f.commit_entries("main", &[], entries, "one", "100 +0000");
        f
    }

    fn names(listing: &DirListing) -> Vec<(&str, EntryType)> {
        listing.entries.iter().map(|entry| (entry.name.as_str(), entry.kind)).collect()
    }

    #[test]
    fn directories_list_first_and_page_by_name() {
        let f = fixture();
        let Contents::Dir(root) = contents(&f.repo, None, "", None, 100).unwrap() else { panic!() };
        assert_eq!(
            names(&root),
            [
                ("a-dir", EntryType::Dir),
                ("src", EntryType::Dir),
                ("README.md", EntryType::File),
                ("latin1.txt", EntryType::File),
                ("link", EntryType::Symlink),
                ("logo.png", EntryType::File),
                ("readme.txt", EntryType::File),
                ("run.sh", EntryType::Executable),
                ("vendor", EntryType::Submodule),
            ]
        );
        assert_eq!(root.entries[2].size, Some(8));
        assert_eq!(root.entries[0].size, None);
        assert!(root.next.is_none());

        let Contents::Dir(first) = contents(&f.repo, None, "", None, 3).unwrap() else { panic!() };
        let Contents::Dir(second) = contents(&f.repo, None, "", first.next.as_deref(), 3).unwrap() else {
            panic!()
        };
        assert_eq!(names(&first).len(), 3);
        assert_eq!(second.entries[0].name, "latin1.txt");

        let Contents::Dir(src) = contents(&f.repo, Some("main"), "/src/", None, 100).unwrap() else {
            panic!()
        };
        assert_eq!(src.path, "src");
        assert_eq!(src.entries[1].path, "src/main.rs");
    }

    #[test]
    fn files_come_with_text_unless_binary() {
        let f = fixture();
        let file = |path: &str| match contents(&f.repo, None, path, None, 100).unwrap() {
            Contents::File(file) => file,
            other => panic!("{other:?}"),
        };
        let main = file("src/main.rs");
        assert_eq!((main.size, main.binary, main.text.as_deref()), (13, false, Some("fn main() {}\n")));
        assert!(file("run.sh").executable);
        let png = file("logo.png");
        assert!(png.binary && png.text.is_none());
        let latin1 = file("latin1.txt");
        assert!(latin1.binary && latin1.text.is_none());
        assert_eq!(file("a-dir/f").text.as_deref(), Some(""));

        let Contents::Symlink(link) = contents(&f.repo, None, "link", None, 100).unwrap() else { panic!() };
        assert_eq!(link.target, "src/main.rs");
        let Contents::Submodule(vendor) = contents(&f.repo, None, "vendor", None, 100).unwrap() else {
            panic!()
        };
        assert_eq!(vendor.commit, "0123456789abcdef0123456789abcdef01234567");
        assert!(matches!(contents(&f.repo, None, "nope", None, 100), Err(Error::PathNotFound(_))));
        assert!(matches!(contents(&f.repo, Some("nope"), "", None, 100), Err(Error::RevisionNotFound(_))));
    }

    #[test]
    fn large_files_come_without_their_text() {
        let f = Fixture::new();
        let big = "x".repeat(TEXT_LIMIT as usize + 1);
        f.commit("main", &[], &[("big.txt", &big)], "big", "100 +0000");
        let Contents::File(file) = contents(&f.repo, None, "big.txt", None, 100).unwrap() else { panic!() };
        assert_eq!((file.size, file.binary, file.text), (TEXT_LIMIT + 1, false, None));
    }

    #[test]
    fn the_readme_prefers_markdown() {
        let f = fixture();
        let root = readme(&f.repo, None, "").unwrap().unwrap();
        assert_eq!((root.path.as_str(), root.text.as_deref()), ("README.md", Some("# Hello\n")));
        assert!(readme(&f.repo, None, "src").unwrap().is_none());
        assert!(matches!(readme(&f.repo, None, "nope"), Err(Error::PathNotFound(_))));
        assert_eq!(readme_rank("ReadMe.rst"), Some(4));
        assert_eq!(readme_rank("readmes"), None);
    }

    #[test]
    fn files_and_symlinks_open_for_reading() {
        let f = fixture();
        let read = |path: &str| {
            let (_, mut reader) = open_file(&f.repo, None, path).unwrap();
            let mut out = Vec::new();
            reader.read_to_end(&mut out).unwrap();
            out
        };
        assert_eq!(read("src/main.rs"), b"fn main() {}\n");
        assert_eq!(read("link"), b"src/main.rs");
        assert!(matches!(open_file(&f.repo, None, "src"), Err(Error::PathNotFound(_))));
        assert!(matches!(open_file(&f.repo, None, "vendor"), Err(Error::PathNotFound(_))));
    }
}
