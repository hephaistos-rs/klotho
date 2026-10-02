//! Read-only views of a repository, shaped for the web UI and API.
//!
//! A `?ref=` is a branch name, a tag name or a commit ID, nothing else: no
//! `HEAD~3`, `@{…}` or `:/message` searches. `None` means the default branch.

use gix::ObjectId;
use gix::objs::tree::EntryKind;
use gix::revision::walk::Sorting;
use gix::traverse::commit::simple::CommitTimeOrder;
use serde::{Serialize, Serializer};

use crate::{Error, Result};

#[derive(Debug, Serialize)]
pub struct RepoInfo {
    /// The branch HEAD points at, e.g. `main`, even if it has no commits yet.
    pub default_branch: Option<String>,
    pub empty: bool,
}

/// One page of a collection, with the cursor for the next (FR-API-020, 021).
#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    /// Opaque: pass it back to get the next page. `None` on the last page.
    pub next: Option<String>,
}

/// A time as git records it: the instant, and the UTC offset of whoever wrote
/// it. Serialized as RFC 3339 with that offset kept (FR-API-011).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitTime {
    pub timestamp: jiff::Timestamp,
    pub offset: jiff::tz::Offset,
}

impl GitTime {
    fn from_gix(time: gix::date::Time) -> Self {
        Self {
            timestamp: jiff::Timestamp::from_second(time.seconds).unwrap_or(jiff::Timestamp::UNIX_EPOCH),
            offset: jiff::tz::Offset::from_seconds(time.offset).unwrap_or(jiff::tz::Offset::UTC),
        }
    }
}

impl std::fmt::Display for GitTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.timestamp.display_with_offset(self.offset).fmt(f)
    }
}

impl Serialize for GitTime {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// An author, committer or tagger.
#[derive(Debug, Clone, Serialize)]
pub struct Signature {
    pub name: String,
    pub email: String,
    pub date: GitTime,
}

impl Signature {
    fn from_gix(signature: gix::actor::SignatureRef<'_>) -> Self {
        let signature = signature.trim();
        Self {
            name: signature.name.to_string(),
            email: signature.email.to_string(),
            date: GitTime::from_gix(signature.time().unwrap_or_default()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CommitInfo {
    /// The full ID, never abbreviated (FR-API-012).
    pub id: String,
    pub parents: Vec<String>,
    pub author: Signature,
    pub committer: Signature,
    /// The first line of the message.
    pub summary: String,
    pub message: String,
}

impl CommitInfo {
    fn from_commit(commit: &gix::Commit<'_>) -> Result<Self> {
        let message = commit.message_raw().map_err(Error::git)?;
        let summary = commit.message().map_err(Error::git)?.summary();
        Ok(Self {
            id: commit.id.to_string(),
            parents: commit.parent_ids().map(|id| id.to_string()).collect(),
            author: Signature::from_gix(commit.author().map_err(Error::git)?),
            committer: Signature::from_gix(commit.committer().map_err(Error::git)?),
            summary: summary.to_string(),
            message: message.to_string(),
        })
    }

    fn load(repo: &gix::Repository, id: ObjectId) -> Result<Self> {
        Self::from_commit(&repo.find_commit(id).map_err(Error::git)?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RefKind {
    Branch,
    Tag,
    Commit,
}

/// What a `?ref=` names, and the commit it comes to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RefTarget {
    pub kind: RefKind,
    /// The branch or tag name, or the full commit ID.
    pub name: String,
    pub commit: String,
}

/// A UI `ref/path` spec, split.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Resolved {
    #[serde(rename = "ref")]
    pub target: RefTarget,
    /// Relative to the root, without leading or trailing `/`. Empty for the root.
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BranchInfo {
    pub name: String,
    /// Whether HEAD points at it.
    pub default: bool,
    pub commit: CommitInfo,
}

#[derive(Debug, Clone, Serialize)]
pub struct TagInfo {
    pub name: String,
    /// The object the tag ref points at: a tag object for an annotated tag.
    pub id: String,
    /// Set for annotated tags.
    pub annotation: Option<Annotation>,
    /// The commit the tag comes to, if it names one rather than a tree or blob.
    pub commit: Option<CommitInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Annotation {
    pub tagger: Option<Signature>,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct TreeEntryInfo {
    pub name: String,
    pub kind: &'static str,
    pub id: String,
}

impl RepoInfo {
    pub fn read(repo: &gix::Repository) -> Result<Self> {
        let head = repo.head().map_err(Error::git)?;
        let default_branch = head.referent_name().map(|name| name.shorten().to_string());
        Ok(Self { default_branch, empty: head.is_unborn() })
    }
}

/// The commit a `?ref=` comes to (see the module docs). A branch wins over a
/// tag of the same name, as in git.
pub fn resolve_ref(repo: &gix::Repository, name: Option<&str>) -> Result<RefTarget> {
    let Some(name) = name.filter(|name| !name.is_empty()) else {
        return resolve_head(repo);
    };
    if let Some(target) = find_named(repo, name)? {
        return Ok(target);
    }
    if is_hex_prefix(repo, name)
        && let Ok(id) = repo.rev_parse_single(name)
        && let Ok(commit) = id.object().map_err(Error::git)?.peel_to_commit()
    {
        let commit = commit.id.to_string();
        return Ok(RefTarget { kind: RefKind::Commit, name: commit.clone(), commit });
    }
    Err(Error::RevisionNotFound(name.to_owned()))
}

/// Splits a UI spec such as `feature/login/src/main.rs` into a ref and a path.
/// The longest branch or tag that the spec starts with wins, as on GitHub.
/// Failing that, the first segment may be a commit ID. An empty spec is the
/// default branch's root.
pub fn resolve(repo: &gix::Repository, spec: &str) -> Result<Resolved> {
    let spec = spec.trim_matches('/');
    if spec.is_empty() {
        return Ok(Resolved { target: resolve_head(repo)?, path: String::new() });
    }
    // Ends of each segment: try `a/b/c`, then `a/b`, then `a`. One lookup each,
    // however many refs the repository has.
    let ends: Vec<usize> =
        spec.match_indices('/').map(|(at, _)| at).chain(std::iter::once(spec.len())).collect();
    for &end in ends.iter().rev() {
        if let Some(target) = find_named(repo, &spec[..end])? {
            return Ok(Resolved { target, path: spec[end..].trim_start_matches('/').to_owned() });
        }
    }
    let first = &spec[..ends[0]];
    let target = resolve_ref(repo, Some(first)).map_err(|_| Error::RevisionNotFound(spec.to_owned()))?;
    Ok(Resolved { target, path: spec[ends[0]..].trim_start_matches('/').to_owned() })
}

fn resolve_head(repo: &gix::Repository) -> Result<RefTarget> {
    let not_found = || Error::RevisionNotFound("HEAD".to_owned());
    let mut head = repo.head().map_err(Error::git)?;
    let name = head.referent_name().map(|name| name.shorten().to_string());
    let commit = head.peel_to_commit().map_err(|_| not_found())?.id.to_string();
    Ok(match name {
        Some(name) => RefTarget { kind: RefKind::Branch, name, commit },
        None => RefTarget { kind: RefKind::Commit, name: commit.clone(), commit },
    })
}

/// The branch, or else the tag, called `name`, if it comes to a commit.
fn find_named(repo: &gix::Repository, name: &str) -> Result<Option<RefTarget>> {
    for (kind, prefix) in [(RefKind::Branch, "refs/heads/"), (RefKind::Tag, "refs/tags/")] {
        if let Some(id) = find_ref(repo, &format!("{prefix}{name}"))?
            && let Some(commit) = peel_to_commit(repo, id)?
        {
            return Ok(Some(RefTarget { kind, name: name.to_owned(), commit: commit.to_string() }));
        }
    }
    Ok(None)
}

/// What the ref `full_name` points at, without peeling. An invalid name is
/// simply not found.
fn find_ref(repo: &gix::Repository, full_name: &str) -> Result<Option<ObjectId>> {
    let Ok(Some(mut reference)) = repo.try_find_reference(full_name) else {
        return Ok(None);
    };
    Ok(Some(reference.follow_to_object().map_err(Error::git)?.detach()))
}

fn peel_to_commit(repo: &gix::Repository, id: ObjectId) -> Result<Option<ObjectId>> {
    let object = repo.find_object(id).map_err(Error::git)?;
    Ok(object.peel_to_commit().ok().map(|commit| commit.id))
}

fn is_hex_prefix(repo: &gix::Repository, name: &str) -> bool {
    (4..=repo.object_hash().len_in_hex()).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Names under `prefix` (e.g. `refs/heads/`), shortened, sorted, after `after`.
fn ref_names(repo: &gix::Repository, prefix: &str, after: Option<&str>) -> Result<Vec<String>> {
    let platform = repo.references().map_err(Error::git)?;
    let mut names = Vec::new();
    for reference in platform.prefixed(prefix).map_err(Error::git)? {
        let reference = reference.map_err(Error::Git)?;
        let full = reference.name().as_bstr().to_string();
        let name = full[prefix.len()..].to_owned();
        if after.is_none_or(|after| name.as_str() > after) {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

/// Cuts `names` to one page, and gives the cursor to the next one.
fn page_of_names(mut names: Vec<String>, limit: usize) -> (Vec<String>, Option<String>) {
    if names.len() <= limit {
        return (names, None);
    }
    names.truncate(limit);
    let next = names.last().cloned();
    (names, next)
}

/// Branches by name, `limit` at a time. The cursor is the last name seen.
pub fn branches(repo: &gix::Repository, after: Option<&str>, limit: usize) -> Result<Page<BranchInfo>> {
    let (names, next) = page_of_names(ref_names(repo, "refs/heads/", after)?, limit);
    let items = names.iter().map(|name| branch(repo, name)).collect::<Result<_>>()?;
    Ok(Page { items, next })
}

pub fn branch(repo: &gix::Repository, name: &str) -> Result<BranchInfo> {
    let not_found = || Error::RevisionNotFound(name.to_owned());
    let id = find_ref(repo, &format!("refs/heads/{name}"))?.ok_or_else(not_found)?;
    let commit = peel_to_commit(repo, id)?.ok_or_else(not_found)?;
    let head = repo.head().map_err(Error::git)?;
    let default = head.referent_name().is_some_and(|head| head.shorten() == name);
    Ok(BranchInfo { name: name.to_owned(), default, commit: CommitInfo::load(repo, commit)? })
}

/// Tags by name, `limit` at a time. The cursor is the last name seen.
pub fn tags(repo: &gix::Repository, after: Option<&str>, limit: usize) -> Result<Page<TagInfo>> {
    let (names, next) = page_of_names(ref_names(repo, "refs/tags/", after)?, limit);
    let items = names.iter().map(|name| tag(repo, name)).collect::<Result<_>>()?;
    Ok(Page { items, next })
}

pub fn tag(repo: &gix::Repository, name: &str) -> Result<TagInfo> {
    let id = find_ref(repo, &format!("refs/tags/{name}"))?
        .ok_or_else(|| Error::RevisionNotFound(name.to_owned()))?;
    let object = repo.find_object(id).map_err(Error::git)?;
    let annotation = match object.kind {
        gix::object::Kind::Tag => {
            let tag = object.clone().into_tag();
            let decoded = tag.decode().map_err(Error::git)?;
            Some(Annotation {
                tagger: tag.tagger().map_err(Error::git)?.map(Signature::from_gix),
                message: decoded.message.to_string(),
            })
        }
        _ => None,
    };
    let commit = match object.peel_to_commit() {
        Ok(commit) => Some(CommitInfo::from_commit(&commit)?),
        Err(_) => None,
    };
    Ok(TagInfo { name: name.to_owned(), id: id.to_string(), annotation, commit })
}

/// The commit log: commits reachable from `rev`, newest first by commit time,
/// `limit` at a time. With a `path`, only commits that changed it: those whose
/// entry at `path` differs from that of every parent.
///
/// The cursor names the commit the walk started from and how far it got, so
/// later pages don't shift when the branch moves on (FR-API-021). A page costs
/// time in proportion to its depth, not to the length of history
/// (NFR-PERF-014).
pub fn log(
    repo: &gix::Repository,
    rev: Option<&str>,
    path: &str,
    cursor: Option<&str>,
    limit: usize,
) -> Result<Page<CommitInfo>> {
    let (tip, skip) = match cursor {
        Some(cursor) => parse_log_cursor(repo, cursor)?,
        None if rev.is_none_or(str::is_empty) && repo.head().map_err(Error::git)?.is_unborn() => {
            return Ok(Page { items: Vec::new(), next: None });
        }
        None => (ObjectId::from_hex(resolve_ref(repo, rev)?.commit.as_bytes()).map_err(Error::git)?, 0),
    };
    let path = path.trim_matches('/');
    let walk = repo
        .rev_walk([tip])
        .sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst))
        .all()
        .map_err(Error::git)?;

    let (mut items, mut seen, mut more) = (Vec::new(), 0, false);
    for info in walk {
        let commit = info.map_err(Error::git)?.object().map_err(Error::git)?;
        if !path.is_empty() && !changes_path(repo, &commit, path)? {
            continue;
        }
        seen += 1;
        if seen <= skip {
            continue;
        }
        if items.len() == limit {
            more = true;
            break;
        }
        items.push(CommitInfo::from_commit(&commit)?);
    }
    Ok(Page { items, next: more.then(|| format!("{tip}.{}", skip + limit)) })
}

fn parse_log_cursor(repo: &gix::Repository, cursor: &str) -> Result<(ObjectId, usize)> {
    let invalid = || Error::InvalidCursor(cursor.to_owned());
    let (tip, skip) = cursor.split_once('.').ok_or_else(invalid)?;
    let tip = ObjectId::from_hex(tip.as_bytes()).map_err(|_| invalid())?;
    let skip = skip.parse().map_err(|_| invalid())?;
    repo.find_commit(tip).map_err(|_| invalid())?;
    Ok((tip, skip))
}

/// Whether `commit` changed `path`: its entry there (or its absence) differs
/// from every parent's.
fn changes_path(repo: &gix::Repository, commit: &gix::Commit<'_>, path: &str) -> Result<bool> {
    let entry_at = |commit: &gix::Commit<'_>| -> Result<Option<(ObjectId, gix::object::tree::EntryMode)>> {
        let tree = commit.tree().map_err(Error::git)?;
        let entry = tree.lookup_entry_by_path(path).map_err(Error::git)?;
        Ok(entry.map(|entry| (entry.object_id(), entry.mode())))
    };
    let mine = entry_at(commit)?;
    let mut parents = commit.parent_ids().peekable();
    if parents.peek().is_none() {
        return Ok(mine.is_some());
    }
    for parent in parents {
        let parent = repo.find_commit(parent).map_err(Error::git)?;
        if entry_at(&parent)? == mine {
            return Ok(false);
        }
    }
    Ok(true)
}

impl TreeEntryInfo {
    /// The entries of the directory at `path` (empty for the root) in `rev`.
    pub fn list(repo: &gix::Repository, rev: Option<&str>, path: &str) -> Result<Vec<Self>> {
        let root = commit_for(repo, rev)?.tree().map_err(Error::git)?;
        let tree = if path.is_empty() {
            root
        } else {
            let entry = root
                .lookup_entry_by_path(path)
                .map_err(Error::git)?
                .filter(|entry| entry.mode().is_tree())
                .ok_or_else(|| Error::PathNotFound(path.to_owned()))?;
            entry.object().map_err(Error::git)?.try_into_tree().map_err(Error::git)?
        };

        tree.iter()
            .map(|entry| {
                let entry = entry.map_err(Error::git)?;
                Ok(Self {
                    name: entry.filename().to_string(),
                    kind: kind_name(entry.mode().kind()),
                    id: entry.oid().to_string(),
                })
            })
            .collect()
    }
}

/// The raw contents of the file at `path` in `rev`.
pub fn read_blob(repo: &gix::Repository, rev: Option<&str>, path: &str) -> Result<Vec<u8>> {
    let root = commit_for(repo, rev)?.tree().map_err(Error::git)?;
    let entry = root
        .lookup_entry_by_path(path)
        .map_err(Error::git)?
        .filter(|entry| entry.mode().is_blob())
        .ok_or_else(|| Error::PathNotFound(path.to_owned()))?;
    let blob = entry.object().map_err(Error::git)?;
    Ok(blob.detach().data)
}

fn commit_for<'repo>(repo: &'repo gix::Repository, rev: Option<&str>) -> Result<gix::Commit<'repo>> {
    let target = resolve_ref(repo, rev)?;
    let id = ObjectId::from_hex(target.commit.as_bytes()).map_err(Error::git)?;
    repo.find_commit(id).map_err(Error::git)
}

fn kind_name(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Tree => "tree",
        EntryKind::Blob => "blob",
        EntryKind::BlobExecutable => "executable",
        EntryKind::Link => "symlink",
        EntryKind::Commit => "submodule",
    }
}

#[cfg(test)]
mod tests {
    use gix::refs::transaction::PreviousValue;

    use super::*;

    /// A bare repository built in-process, with commits at chosen times.
    struct Fixture {
        _dir: tempfile::TempDir,
        repo: gix::Repository,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("r.git");
            gix::init_bare(&path).unwrap();
            std::fs::write(path.join("HEAD"), "ref: refs/heads/main\n").unwrap();
            let repo = gix::open(&path).unwrap();
            Self { _dir: dir, repo }
        }

        /// A commit on `branch` setting `files` on top of `parents[0]`'s tree.
        fn commit(
            &self,
            branch: &str,
            parents: &[ObjectId],
            files: &[(&str, &str)],
            message: &str,
            time: &str,
        ) -> ObjectId {
            let base = match parents.first() {
                Some(&parent) => self.repo.find_commit(parent).unwrap().tree_id().unwrap().detach(),
                None => ObjectId::empty_tree(self.repo.object_hash()),
            };
            let mut editor = self.repo.edit_tree(base).unwrap();
            for (path, content) in files {
                let blob = self.repo.write_blob(content).unwrap();
                editor.upsert(*path, EntryKind::Blob, blob).unwrap();
            }
            let tree = editor.write().unwrap();
            let who = gix::actor::SignatureRef { name: "Ann".into(), email: "ann@example.com".into(), time };
            let commit = self.repo.new_commit_as(who, who, message, tree, parents.iter().copied()).unwrap();
            let id = commit.id;
            self.repo.reference(format!("refs/heads/{branch}"), id, PreviousValue::Any, "test").unwrap();
            id
        }

        fn log(&self, rev: Option<&str>, path: &str, cursor: Option<&str>, limit: usize) -> Page<CommitInfo> {
            log(&self.repo, rev, path, cursor, limit).unwrap()
        }
    }

    fn summaries(page: &Page<CommitInfo>) -> Vec<&str> {
        page.items.iter().map(|commit| commit.summary.as_str()).collect()
    }

    #[test]
    fn commit_times_keep_their_offset() {
        let f = Fixture::new();
        f.commit("main", &[], &[("a", "1")], "first", "1700000000 +0230");
        let page = f.log(None, "", None, 10);
        let json = serde_json::to_value(&page.items[0]).unwrap();
        assert_eq!(json["author"]["date"], "2023-11-15T00:43:20+02:30");
        assert_eq!(json["committer"]["date"], "2023-11-15T00:43:20+02:30");
        assert_eq!(json["id"].as_str().unwrap().len(), 40);
    }

    #[test]
    fn the_log_pages_with_a_cursor_that_survives_new_commits() {
        let f = Fixture::new();
        let mut parent = Vec::new();
        for n in 0..5 {
            let id = f.commit(
                "main",
                &parent,
                &[("a", &n.to_string())],
                &format!("c{n}"),
                &format!("{} +0000", 1000 + n),
            );
            parent = vec![id];
        }
        let first = f.log(Some("main"), "", None, 2);
        assert_eq!(summaries(&first), ["c4", "c3"]);
        // The branch moves on; the next page carries on where the first stopped.
        f.commit("main", &parent, &[("a", "new")], "c5", "2000 +0000");
        let second = f.log(Some("main"), "", first.next.as_deref(), 2);
        assert_eq!(summaries(&second), ["c2", "c1"]);
        let third = f.log(Some("main"), "", second.next.as_deref(), 2);
        assert_eq!(summaries(&third), ["c0"]);
        assert!(third.next.is_none());

        assert!(matches!(log(&f.repo, None, "", Some("nonsense"), 2), Err(Error::InvalidCursor(_))));
    }

    #[test]
    fn the_log_follows_both_sides_of_a_merge_and_filters_by_path() {
        let f = Fixture::new();
        let base = f.commit("main", &[], &[("src/a", "1"), ("b", "1")], "base", "100 +0000");
        let side = f.commit("side", &[base], &[("src/a", "2")], "side changes a", "200 +0000");
        let main = f.commit("main", &[base], &[("b", "2")], "main changes b", "300 +0000");
        f.commit("main", &[main, side], &[("src/a", "2")], "merge", "400 +0000");

        assert_eq!(
            summaries(&f.log(None, "", None, 10)),
            ["merge", "main changes b", "side changes a", "base"]
        );
        // The merge took `src/a` from one parent unchanged, so it didn't change it.
        assert_eq!(summaries(&f.log(None, "src/a", None, 10)), ["side changes a", "base"]);
        assert_eq!(summaries(&f.log(None, "/src/", None, 10)), ["side changes a", "base"]);
        assert_eq!(summaries(&f.log(None, "b", None, 10)), ["main changes b", "base"]);
        assert!(f.log(None, "nothing", None, 10).items.is_empty());
    }

    #[test]
    fn an_empty_repository_has_an_empty_log() {
        let f = Fixture::new();
        assert!(f.log(None, "", None, 10).items.is_empty());
        assert!(matches!(log(&f.repo, Some("main"), "", None, 10), Err(Error::RevisionNotFound(_))));
    }

    #[test]
    fn refs_are_branches_tags_or_commit_ids_only() {
        let f = Fixture::new();
        let one = f.commit("main", &[], &[("a", "1")], "one", "100 +0000");
        let two = f.commit("main", &[one], &[("a", "2")], "two", "200 +0000");
        f.repo.reference("refs/tags/v1", one, PreviousValue::Any, "test").unwrap();
        let resolve =
            |name: Option<&str>| resolve_ref(&f.repo, name).map(|target| (target.kind, target.commit));

        assert_eq!(resolve(None).unwrap(), (RefKind::Branch, two.to_string()));
        assert_eq!(resolve(Some("main")).unwrap(), (RefKind::Branch, two.to_string()));
        assert_eq!(resolve(Some("v1")).unwrap(), (RefKind::Tag, one.to_string()));
        assert_eq!(resolve(Some(&one.to_string()[..7])).unwrap(), (RefKind::Commit, one.to_string()));
        for bad in ["main~1", "HEAD^", ":/one", "main@{0}", "nope", "../../etc"] {
            assert!(matches!(resolve(Some(bad)), Err(Error::RevisionNotFound(_))), "{bad}");
        }
    }

    #[test]
    fn specs_split_at_the_longest_branch_or_tag() {
        let f = Fixture::new();
        let one = f.commit("main", &[], &[("src/main.rs", "fn main() {}")], "one", "100 +0000");
        f.commit("feature/login", &[one], &[("x", "1")], "two", "200 +0000");
        f.repo.reference("refs/tags/feature/login/v2", one, PreviousValue::Any, "test").unwrap();
        let split = |spec: &str| {
            let resolved = resolve(&f.repo, spec).unwrap();
            (resolved.target.kind, resolved.target.name, resolved.path)
        };

        assert_eq!(split(""), (RefKind::Branch, "main".to_owned(), String::new()));
        assert_eq!(split("main/src/main.rs"), (RefKind::Branch, "main".to_owned(), "src/main.rs".to_owned()));
        assert_eq!(
            split("feature/login/src"),
            (RefKind::Branch, "feature/login".to_owned(), "src".to_owned())
        );
        assert_eq!(
            split("feature/login/v2/src/"),
            (RefKind::Tag, "feature/login/v2".to_owned(), "src".to_owned())
        );
        assert_eq!(split(&format!("{one}/src")), (RefKind::Commit, one.to_string(), "src".to_owned()));
        assert!(matches!(resolve(&f.repo, "nope/src"), Err(Error::RevisionNotFound(_))));
    }

    #[test]
    fn branches_and_tags_page_by_name() {
        let f = Fixture::new();
        let one = f.commit("main", &[], &[("a", "1")], "one", "100 +0000");
        f.commit("b", &[one], &[("a", "2")], "two", "200 +0000");
        f.commit("a", &[one], &[("a", "3")], "three", "300 +0000");
        let tagger = gix::actor::SignatureRef {
            name: "Ann".into(),
            email: "ann@example.com".into(),
            time: "500 -0100",
        };
        f.repo
            .tag("v1", one, gix::object::Kind::Commit, Some(tagger), "Release 1", PreviousValue::Any)
            .unwrap();
        f.repo.reference("refs/tags/light", one, PreviousValue::Any, "test").unwrap();
        let tree = f.repo.find_commit(one).unwrap().tree_id().unwrap().detach();
        f.repo.reference("refs/tags/tree", tree, PreviousValue::Any, "test").unwrap();

        let first = branches(&f.repo, None, 2).unwrap();
        let names: Vec<_> =
            first.items.iter().map(|b| (b.name.as_str(), b.default, b.commit.summary.as_str())).collect();
        assert_eq!(names, [("a", false, "three"), ("b", false, "two")]);
        let rest = branches(&f.repo, first.next.as_deref(), 2).unwrap();
        assert_eq!(rest.items[0].name, "main");
        assert!(rest.items[0].default && rest.next.is_none());

        let tags = tags(&f.repo, None, 10).unwrap();
        let names: Vec<_> = tags.items.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["light", "tree", "v1"]);
        assert!(tags.items[0].annotation.is_none());
        assert_eq!(tags.items[0].commit.as_ref().unwrap().id, one.to_string());
        assert!(tags.items[1].commit.is_none());
        let v1 = &tags.items[2];
        assert_ne!(v1.id, one.to_string(), "an annotated tag points at its tag object");
        let annotation = v1.annotation.as_ref().unwrap();
        assert_eq!(annotation.message.trim(), "Release 1");
        assert_eq!(annotation.tagger.as_ref().unwrap().date.to_string(), "1969-12-31T23:08:20-01:00");
        assert_eq!(v1.commit.as_ref().unwrap().id, one.to_string());

        assert!(matches!(branch(&f.repo, "nope"), Err(Error::RevisionNotFound(_))));
        assert!(matches!(tag(&f.repo, "nope"), Err(Error::RevisionNotFound(_))));
    }
}
