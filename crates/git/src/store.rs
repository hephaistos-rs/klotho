use std::path::{Path, PathBuf};

use crate::{Error, RepoName, Result};

/// A directory of bare repositories, stored as `<root>/<name>.git`.
#[derive(Debug, Clone)]
pub struct RepoStore {
    root: PathBuf,
}

impl RepoStore {
    /// The branch HEAD points at in new repos. gix otherwise uses `master`.
    pub const DEFAULT_BRANCH: &str = "main";

    /// Uses `root` as the storage directory, creating it if needed.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let root = std::path::absolute(root.into())?;
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The on-disk location of `name`, whether or not it exists yet.
    pub fn path(&self, name: &RepoName) -> PathBuf {
        self.root.join(format!("{name}.git"))
    }

    pub fn exists(&self, name: &RepoName) -> bool {
        self.path(name).is_dir()
    }

    pub fn create(&self, name: &RepoName) -> Result<gix::Repository> {
        let path = self.path(name);
        if path.exists() {
            return Err(Error::RepoExists(name.to_string()));
        }
        let repo = gix::init_bare(path).map_err(Error::git)?;
        set_default_branch(&repo, Self::DEFAULT_BRANCH)?;
        Ok(repo)
    }

    pub fn open(&self, name: &RepoName) -> Result<gix::Repository> {
        if !self.exists(name) {
            return Err(Error::RepoNotFound(name.to_string()));
        }
        gix::open(self.path(name)).map_err(Error::git)
    }

    /// All repositories in the store, sorted by name. Entries that aren't
    /// `<valid-name>.git` directories are skipped, as are ones whose name isn't
    /// already normalized (e.g. `Demo.git` added by hand), since [`Self::open`]
    /// would look for `demo.git` and not find them on case-sensitive filesystems.
    pub fn list(&self) -> Result<Vec<RepoName>> {
        let mut names = Vec::new();
        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            let file_name = entry.file_name();
            let Some(stem) = file_name.to_str().and_then(|n| n.strip_suffix(".git")) else {
                continue;
            };
            if !entry.file_type()?.is_dir() {
                continue;
            }
            if let Ok(name) = stem.parse::<RepoName>()
                && name.as_str() == stem
            {
                names.push(name);
            }
        }
        names.sort();
        Ok(names)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_list_and_open() {
        let root = std::env::temp_dir().join(format!("klotho-store-test-{}", std::process::id()));
        let store = RepoStore::new(&root).unwrap();
        let name: RepoName = "demo".parse().unwrap();

        let repo = store.create(&name).unwrap();
        assert!(repo.is_bare());
        let head = repo.head().unwrap();
        assert!(head.is_unborn());
        assert_eq!(head.referent_name().unwrap().as_bstr(), "refs/heads/main");

        assert!(matches!(store.create(&name), Err(Error::RepoExists(_))));
        assert!(matches!(store.create(&"Demo".parse().unwrap()), Err(Error::RepoExists(_))));
        assert_eq!(store.list().unwrap(), vec![name.clone()]);
        assert!(store.open(&name).is_ok());
        assert!(matches!(store.open(&"missing".parse().unwrap()), Err(Error::RepoNotFound(_))));

        std::fs::remove_dir_all(root).unwrap();
    }
}
