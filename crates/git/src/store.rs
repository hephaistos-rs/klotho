use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// A repository's immutable ID, assigned by the metadata store (FR-STOR-003).
pub type RepoId = i64;

/// Bare repositories on disk, addressed by ID, never by name (FR-STOR-004):
/// `<root>/<h[0:2]>/<h[2:4]>/<h>.git` with `h = hex(sha256(id))`.
///
/// Names never reach the filesystem, so renames don't move data (FR-STOR-006),
/// and case-insensitive filesystems and Windows device names like `con` stop
/// mattering (FR-STOR-007, 008).
#[derive(Debug, Clone)]
pub struct RepoStore {
    root: PathBuf,
}

/// Where repositories are initialised before being renamed into place.
const TMP_DIR: &str = ".tmp";
/// Where leftovers of interrupted operations are moved instead of deleted.
const ORPHANED_DIR: &str = ".orphaned";
/// Where repositories go on their way to being deleted.
const TRASH_DIR: &str = ".trash";

impl RepoStore {
    /// The branch HEAD points at in new repos. gix otherwise uses `master`.
    pub const DEFAULT_BRANCH: &str = "main";

    /// Uses `root` as the storage directory, creating it if needed. Leftovers of
    /// creates and deletes interrupted by a crash are removed.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let root = std::path::absolute(root.into())?;
        std::fs::create_dir_all(&root)?;
        for leftover in [TMP_DIR, TRASH_DIR] {
            match std::fs::remove_dir_all(root.join(leftover)) {
                Err(err) if err.kind() != std::io::ErrorKind::NotFound => return Err(err.into()),
                _ => {}
            }
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The on-disk location of repository `id`, whether or not it exists yet.
    pub fn path(&self, id: RepoId) -> PathBuf {
        self.root.join(Self::relative_path(id))
    }

    /// [`Self::path`] relative to the root, e.g. `6b/86/6b86b2….git`.
    pub fn relative_path(id: RepoId) -> PathBuf {
        let hash = hex(&Sha256::digest(id.to_string().as_bytes()));
        [&hash[0..2], &hash[2..4], &format!("{hash}.git")].iter().collect()
    }

    pub fn exists(&self, id: RepoId) -> bool {
        self.path(id).is_dir()
    }

    /// Creates an empty bare repository for `id`. It's initialised in a temporary
    /// directory and renamed into place, so a half-initialised repository is never
    /// visible (FR-STOR-005).
    pub fn create(&self, id: RepoId) -> Result<()> {
        let target = self.path(id);
        if target.exists() {
            return Err(Error::RepoExists(id.to_string()));
        }
        let tmp_root = self.root.join(TMP_DIR);
        std::fs::create_dir_all(&tmp_root)?;
        let tmp = tempfile::Builder::new().prefix("create-").tempdir_in(&tmp_root)?;
        let staged = tmp.path().join("repo.git");
        let repo = gix::init_bare(&staged).map_err(Error::git)?;
        set_default_branch(&repo, Self::DEFAULT_BRANCH)?;
        drop(repo);

        std::fs::create_dir_all(target.parent().expect("repository paths have parents"))?;
        // Renaming onto an existing directory fails on every platform, so a racing
        // create can't be overwritten.
        std::fs::rename(&staged, &target)
            .map_err(|err| if target.exists() { Error::RepoExists(id.to_string()) } else { err.into() })
    }

    pub fn open(&self, id: RepoId) -> Result<gix::Repository> {
        if !self.exists(id) {
            return Err(Error::RepoNotFound(id.to_string()));
        }
        gix::open(self.path(id)).map_err(Error::git)
    }

    /// Moves the directory at `id`'s path out of the way, into `.orphaned/`. For
    /// a directory the metadata store doesn't know, e.g. left by a create that
    /// crashed before its transaction committed. Returns where it went.
    pub fn set_aside(&self, id: RepoId) -> Result<PathBuf> {
        let from = self.path(id);
        let orphaned = self.root.join(ORPHANED_DIR);
        std::fs::create_dir_all(&orphaned)?;
        let stamp = unix_seconds();
        let name = from.file_stem().expect("repository paths have a file name").to_string_lossy();
        let to = orphaned.join(format!("{name}-{stamp}.git"));
        std::fs::rename(&from, &to)?;
        Ok(to)
    }

    /// Moves the bare repository at `relative` (from [`Self::scan`]) to `id`'s path.
    pub fn adopt(&self, relative: &str, id: RepoId) -> Result<()> {
        let from = self.resolve_relative(relative)?;
        if !is_bare_repo(&from) {
            return Err(Error::NotARepository(relative.to_owned()));
        }
        let target = self.path(id);
        if target.exists() {
            return Err(Error::RepoExists(id.to_string()));
        }
        std::fs::create_dir_all(target.parent().expect("repository paths have parents"))?;
        std::fs::rename(&from, &target)?;
        Ok(())
    }

    /// Moves the directory at `relative` back, undoing [`Self::adopt`] when the
    /// metadata change that went with it failed.
    pub fn unadopt(&self, id: RepoId, relative: &str) -> Result<()> {
        let to = self.resolve_relative(relative)?;
        std::fs::rename(self.path(id), to)?;
        Ok(())
    }

    /// Deletes the bare repository at `relative` (from [`Self::scan`]). It's first
    /// renamed into `.trash/`, so a crash halfway leaves nothing half-deleted in
    /// place; the next start removes the trash.
    pub fn delete_relative(&self, relative: &str) -> Result<()> {
        let path = self.resolve_relative(relative)?;
        if !is_bare_repo(&path) {
            return Err(Error::NotARepository(relative.to_owned()));
        }
        let trash = self.root.join(TRASH_DIR);
        std::fs::create_dir_all(&trash)?;
        let tmp = tempfile::Builder::new().prefix("delete-").tempdir_in(&trash)?;
        let staged = tmp.path().join("repo.git");
        std::fs::rename(&path, &staged)?;
        drop(tmp); // removes the directory and everything in it
        Ok(())
    }

    /// Every bare repository under the root, as paths relative to it with `/`
    /// separators: both ID-hashed ones (`ab/cd/<h>.git`) and any others, such as
    /// repositories from before ID-based storage (`demo.git`). Hidden entries
    /// (`.tmp`, `.orphaned`, …) are skipped.
    pub fn scan(&self) -> Result<Vec<String>> {
        let mut found = Vec::new();
        for entry in visible_dirs(&self.root)? {
            let name = entry.file_name().unwrap().to_string_lossy().into_owned();
            if is_hex_pair(&name) {
                for second in visible_dirs(&entry)? {
                    let second_name = second.file_name().unwrap().to_string_lossy().into_owned();
                    for repo in visible_dirs(&second)? {
                        if is_bare_repo(&repo) {
                            let repo_name = repo.file_name().unwrap().to_string_lossy();
                            found.push(format!("{name}/{second_name}/{repo_name}"));
                        }
                    }
                }
            } else if is_bare_repo(&entry) {
                found.push(name);
            }
        }
        found.sort();
        Ok(found)
    }

    /// Turns a `/`-separated path from [`Self::scan`] back into a path under the
    /// root, refusing anything that could point outside it.
    fn resolve_relative(&self, relative: &str) -> Result<PathBuf> {
        let path = Path::new(relative);
        let safe = !relative.is_empty()
            && path
                .components()
                .all(|c| matches!(c, Component::Normal(part) if !part.to_string_lossy().starts_with('.')));
        if !safe {
            return Err(Error::NotARepository(relative.to_owned()));
        }
        Ok(self.root.join(path))
    }
}

/// Points HEAD at `refs/heads/<branch>`, which doesn't need to exist yet.
fn set_default_branch(repo: &gix::Repository, branch: &str) -> Result<()> {
    use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit};

    let target = format!("refs/heads/{branch}").try_into().map_err(Error::git)?;
    repo.edit_reference(RefEdit {
        change: Change::Update {
            log: LogChange::default(),
            expected: PreviousValue::Any,
            new: gix::refs::Target::Symbolic(target),
        },
        name: "HEAD".try_into().map_err(Error::git)?,
        deref: false,
    })
    .map_err(Error::git)?;
    Ok(())
}

/// A bare repository has `HEAD`, `objects/` and `refs/` at its top level.
fn is_bare_repo(dir: &Path) -> bool {
    dir.join("HEAD").is_file() && dir.join("objects").is_dir() && dir.join("refs").is_dir()
}

fn visible_dirs(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() && !entry.file_name().to_string_lossy().starts_with('.') {
            dirs.push(entry.path());
        }
    }
    Ok(dirs)
}

fn is_hex_pair(name: &str) -> bool {
    name.len() == 2 && name.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Seconds since the Unix epoch, for unique names in `.orphaned/`.
fn unix_seconds() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, RepoStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = RepoStore::new(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn path_is_derived_from_the_id_hash() {
        // sha256("1") = 6b86b273ff34fce19d6b804eff5a3f5747ada4eaa22f1d49c01e52ddb7875b4b
        let expected: PathBuf =
            ["6b", "86", "6b86b273ff34fce19d6b804eff5a3f5747ada4eaa22f1d49c01e52ddb7875b4b.git"]
                .iter()
                .collect();
        assert_eq!(RepoStore::relative_path(1), expected);
    }

    #[test]
    fn create_and_open() {
        let (_dir, store) = store();
        store.create(1).unwrap();
        let repo = store.open(1).unwrap();
        assert!(repo.is_bare());
        let head = repo.head().unwrap();
        assert!(head.is_unborn());
        assert_eq!(head.referent_name().unwrap().as_bstr(), "refs/heads/main");

        assert!(matches!(store.create(1), Err(Error::RepoExists(_))));
        assert!(matches!(store.open(2), Err(Error::RepoNotFound(_))));
        // Nothing staged is left behind.
        assert_eq!(std::fs::read_dir(store.root().join(TMP_DIR)).unwrap().count(), 0);
    }

    #[test]
    fn scan_finds_hashed_and_legacy_repositories_but_not_hidden_ones() {
        let (_dir, store) = store();
        store.create(1).unwrap();
        gix::init_bare(store.root().join("demo.git")).unwrap();
        std::fs::create_dir(store.root().join(ORPHANED_DIR)).unwrap();
        gix::init_bare(store.root().join(ORPHANED_DIR).join("x.git")).unwrap();
        std::fs::create_dir(store.root().join("not-a-repo")).unwrap();

        let hashed = RepoStore::relative_path(1).to_string_lossy().replace('\\', "/");
        assert_eq!(store.scan().unwrap(), vec![hashed, "demo.git".to_owned()]);
    }

    #[test]
    fn adopt_moves_a_legacy_repository_to_its_id_path() {
        let (_dir, store) = store();
        gix::init_bare(store.root().join("demo.git")).unwrap();
        store.adopt("demo.git", 7).unwrap();
        assert!(store.exists(7));
        assert!(!store.root().join("demo.git").exists());

        store.unadopt(7, "demo.git").unwrap();
        assert!(store.root().join("demo.git").exists());
    }

    #[test]
    fn relative_paths_cant_escape_the_root() {
        let (_dir, store) = store();
        for bad in ["", "..", "../x.git", "a/../../x.git", ".orphaned/x.git"] {
            assert!(store.adopt(bad, 1).is_err(), "{bad:?}");
            assert!(store.delete_relative(bad).is_err(), "{bad:?}");
        }
        #[cfg(windows)]
        assert!(store.adopt(r"C:\x.git", 1).is_err());
        #[cfg(unix)]
        assert!(store.adopt("/x.git", 1).is_err());
    }

    #[test]
    fn set_aside_moves_an_unknown_directory_out_of_the_way() {
        let (_dir, store) = store();
        store.create(3).unwrap();
        let moved = store.set_aside(3).unwrap();
        assert!(!store.exists(3));
        assert!(moved.starts_with(store.root().join(ORPHANED_DIR)));
        store.create(3).unwrap();
    }

    #[test]
    fn delete_relative_removes_the_repository() {
        let (_dir, store) = store();
        gix::init_bare(store.root().join("old.git")).unwrap();
        store.delete_relative("old.git").unwrap();
        assert!(!store.root().join("old.git").exists());
    }
}
