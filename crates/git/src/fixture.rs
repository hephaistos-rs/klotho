//! Test repositories built in-process, with commits at chosen times.

use gix::ObjectId;
use gix::objs::tree::EntryKind;
use gix::refs::transaction::PreviousValue;

/// A bare repository whose HEAD points at `main`.
pub struct Fixture {
    _dir: tempfile::TempDir,
    pub repo: gix::Repository,
}

impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("r.git");
        gix::init_bare(&path).unwrap();
        // gix points HEAD at `master` unless configured otherwise.
        std::fs::write(path.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        let repo = gix::open(&path).unwrap();
        Self { _dir: dir, repo }
    }

    /// A commit on `branch` setting `files` on top of `parents[0]`'s tree.
    pub fn commit(
        &self,
        branch: &str,
        parents: &[ObjectId],
        files: &[(&str, &str)],
        message: &str,
        time: &str,
    ) -> ObjectId {
        let entries: Vec<_> =
            files.iter().map(|(path, content)| (*path, EntryKind::Blob, content.as_bytes())).collect();
        self.commit_entries(branch, parents, &entries, message, time)
    }

    /// [`Self::commit`] with any kind of entry. A `Commit` entry's content is
    /// the submodule commit ID in hex.
    pub fn commit_entries(
        &self,
        branch: &str,
        parents: &[ObjectId],
        entries: &[(&str, EntryKind, &[u8])],
        message: &str,
        time: &str,
    ) -> ObjectId {
        let base = match parents.first() {
            Some(&parent) => self.repo.find_commit(parent).unwrap().tree_id().unwrap().detach(),
            None => ObjectId::empty_tree(self.repo.object_hash()),
        };
        let mut editor = self.repo.edit_tree(base).unwrap();
        for &(path, kind, content) in entries {
            let id = match kind {
                EntryKind::Commit => ObjectId::from_hex(content).unwrap(),
                _ => self.repo.write_blob(content).unwrap().detach(),
            };
            editor.upsert(path, kind, id).unwrap();
        }
        let tree = editor.write().unwrap();
        let who = gix::actor::SignatureRef { name: "Ann".into(), email: "ann@example.com".into(), time };
        let commit = self.repo.new_commit_as(who, who, message, tree, parents.iter().copied()).unwrap();
        let id = commit.id;
        self.repo.reference(format!("refs/heads/{branch}"), id, PreviousValue::Any, "test").unwrap();
        id
    }
}
