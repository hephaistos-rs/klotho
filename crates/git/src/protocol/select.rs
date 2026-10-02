//! Which commits a fetch sends, including shallow fetches (`deepen`,
//! `deepen-since`, `deepen-not`, `deepen-relative`).
//!
//! Shared by protocol v0/v1 and v2; only the wire format around it differs.

use std::collections::{HashMap, HashSet, VecDeque};

use gix::hash::ObjectId;
use gix::objs::Kind;

use super::{ProtocolError, ServeError};
use crate::Error;

/// Depth given as "infinite" by `git fetch --unshallow`.
const INFINITE_DEPTH: u32 = 0x7fff_ffff;

/// The shallow part of a fetch request.
#[derive(Debug, Default)]
pub struct ShallowRequest {
    /// Commits the client's history stops at (`shallow <id>`).
    pub client: Vec<ObjectId>,
    /// `deepen <n>`: this many commits from each want.
    pub depth: Option<u32>,
    /// `deepen-relative`: count `depth` from the client's shallow commits instead.
    pub relative: bool,
    /// `deepen-since <unix time>`: commits at or after this time.
    pub since: Option<i64>,
    /// `deepen-not <ref>`: commits not reachable from these refs.
    pub not: Vec<String>,
}

impl ShallowRequest {
    /// Takes one request line if it's about shallow fetching. Returns whether it was.
    pub fn parse_line(&mut self, line: &str) -> Result<bool, ProtocolError> {
        if let Some(id) = line.strip_prefix("shallow ") {
            self.client.push(parse_id(id)?);
        } else if let Some(depth) = line.strip_prefix("deepen ") {
            let depth: u32 =
                depth.parse().map_err(|_| ProtocolError::new(format!("invalid deepen {depth:?}")))?;
            if depth == 0 {
                return Err(ProtocolError::new("invalid deepen 0"));
            }
            self.depth = Some(depth);
        } else if let Some(time) = line.strip_prefix("deepen-since ") {
            let time =
                time.parse().map_err(|_| ProtocolError::new(format!("invalid deepen-since {time:?}")))?;
            self.since = Some(time);
        } else if let Some(name) = line.strip_prefix("deepen-not ") {
            self.not.push(name.to_owned());
        } else if line == "deepen-relative" {
            self.relative = true;
        } else {
            return Ok(false);
        }
        Ok(true)
    }

    /// Whether the client asked to change how deep its history goes. The
    /// response then lists the new shallow boundary.
    pub fn deepens(&self) -> bool {
        self.depth.is_some() || self.since.is_some() || !self.not.is_empty()
    }
}

/// What to send.
#[derive(Debug, Default)]
pub struct Selection {
    /// Commits to send. Each brings the objects its tree adds compared to its parents.
    pub commits: Vec<ObjectId>,
    /// The subset of `commits` whose parents the client won't have, so their
    /// whole tree has to be sent.
    pub full_trees: HashSet<ObjectId>,
    /// New shallow commits to report (`shallow <id>`).
    pub shallow: Vec<ObjectId>,
    /// The client's shallow commits whose parents it now gets (`unshallow <id>`).
    pub unshallow: Vec<ObjectId>,
}

/// Works out what a fetch of `wants` sends to a client that has `common`.
///
/// The shallow boundary depends only on the wants and the request, never on
/// `common`: over HTTP the client sends the same wants in every round and
/// keeps the boundary from the first answer.
pub fn select(
    repo: &gix::Repository,
    wants: &[ObjectId],
    common: &[ObjectId],
    shallow: &ShallowRequest,
) -> Result<Selection, ServeError> {
    let mut graph = Graph::new(repo);
    let mut tips = Vec::new();
    for want in wants {
        let object = repo.find_object(*want).map_err(Error::git)?;
        if let Ok(commit) = object.peel_to_kind(Kind::Commit) {
            tips.push(commit.id);
        }
    }
    if tips.is_empty() {
        return Ok(Selection::default());
    }
    if !shallow.deepens() && shallow.client.is_empty() {
        let walk = repo.rev_walk(tips).with_hidden(common.iter().copied()).all().map_err(Error::git)?;
        let commits = walk.map(|info| Ok(info.map_err(Error::git)?.id)).collect::<Result<_, Error>>()?;
        return Ok(Selection { commits, ..Selection::default() });
    }

    let client_shallow: HashSet<ObjectId> =
        shallow.client.iter().copied().filter(|id| graph.is_commit(*id)).collect();
    let has = possessed(&mut graph, common, &client_shallow)?;
    let included = if shallow.deepens() {
        deepen(&mut graph, &tips, &client_shallow, &has, shallow)?
    } else {
        reachable_until(&mut graph, &tips, |id| has.contains(&id))?
    };

    let mut selection = Selection::default();
    for &id in &included.order {
        let parents = graph.parents(id)?.to_vec();
        let may_cut = included.may_cut.as_ref().is_none_or(|may_cut| may_cut.contains(&id));
        let cut = may_cut && parents.iter().any(|parent| !included.set.contains(parent));
        if shallow.deepens() && cut {
            if !client_shallow.contains(&id) {
                selection.shallow.push(id);
            }
        } else if client_shallow.contains(&id) && shallow.deepens() {
            selection.unshallow.push(id);
        }
        if !has.contains(&id) {
            selection.commits.push(id);
            if parents.iter().any(|parent| !included.set.contains(parent) && !has.contains(parent)) {
                selection.full_trees.insert(id);
            }
        }
    }
    Ok(selection)
}

/// Commits in visiting order, plus a set for lookups.
#[derive(Default)]
struct Included {
    order: Vec<ObjectId>,
    set: HashSet<ObjectId>,
    /// The commits that may become shallow, if not all of them.
    may_cut: Option<HashSet<ObjectId>>,
}

impl Included {
    fn insert(&mut self, id: ObjectId) -> bool {
        let new = self.set.insert(id);
        if new {
            self.order.push(id);
        }
        new
    }
}

/// The commits the client has: everything reachable from `common`, except
/// below its own shallow commits.
fn possessed(
    graph: &mut Graph<'_>,
    common: &[ObjectId],
    client_shallow: &HashSet<ObjectId>,
) -> Result<HashSet<ObjectId>, ServeError> {
    let mut seen = HashSet::new();
    let mut queue: VecDeque<ObjectId> = common.iter().chain(client_shallow).copied().collect();
    while let Some(id) = queue.pop_front() {
        // The client has a shallow commit, but none of its parents.
        if !seen.insert(id) || client_shallow.contains(&id) {
            continue;
        }
        queue.extend(graph.parents(id)?.iter().copied());
    }
    Ok(seen)
}

/// Every commit reachable from `starts`, not going past (or including)
/// commits where `stop` is true.
fn reachable_until(
    graph: &mut Graph<'_>,
    starts: &[ObjectId],
    stop: impl Fn(ObjectId) -> bool,
) -> Result<Included, ServeError> {
    let mut included = Included::default();
    let mut queue: VecDeque<ObjectId> = starts.iter().copied().collect();
    while let Some(id) = queue.pop_front() {
        if stop(id) || !included.insert(id) {
            continue;
        }
        queue.extend(graph.parents(id)?.iter().copied());
    }
    Ok(included)
}

/// The commits the client has after a deepening fetch.
fn deepen(
    graph: &mut Graph<'_>,
    tips: &[ObjectId],
    client_shallow: &HashSet<ObjectId>,
    has: &HashSet<ObjectId>,
    request: &ShallowRequest,
) -> Result<Included, ServeError> {
    if request.depth.is_some() && (request.since.is_some() || !request.not.is_empty()) {
        return Err(
            ProtocolError::new("deepen and deepen-since (or deepen-not) cannot be used together").into()
        );
    }
    if let Some(depth) = request.depth {
        if request.relative {
            // Everything new down to the client's shallow commits, then `depth`
            // more generations below them. Only those generations can end in a
            // new shallow boundary.
            let mut included =
                reachable_until(graph, tips, |id| client_shallow.contains(&id) || has.contains(&id))?;
            let mut deeper = Included::default();
            let starts: Vec<(ObjectId, u32)> = client_shallow.iter().map(|&id| (id, 1)).collect();
            by_depth(graph, &mut deeper, starts, depth.saturating_add(1))?;
            for &id in &deeper.order {
                included.insert(id);
            }
            included.may_cut = Some(deeper.set);
            return Ok(included);
        }
        let mut included = Included::default();
        by_depth(graph, &mut included, tips.iter().map(|&id| (id, 1)).collect(), depth)?;
        return Ok(included);
    }

    let mut excluded = HashSet::new();
    for name in &request.not {
        let mut reference = graph
            .repo
            .find_reference(name.as_str())
            .map_err(|_| ProtocolError::new(format!("deepen-not is not a ref: {name}")))?;
        let id = reference.peel_to_id().map_err(Error::git)?.detach();
        excluded.extend(reachable_until(graph, &[id], |_| false)?.set);
    }
    let mut included = Included::default();
    let mut queue: VecDeque<ObjectId> = tips.iter().copied().collect();
    while let Some(id) = queue.pop_front() {
        if excluded.contains(&id) || included.set.contains(&id) {
            continue;
        }
        if let Some(since) = request.since
            && graph.time(id)? < since
        {
            continue;
        }
        included.insert(id);
        queue.extend(graph.parents(id)?.iter().copied());
    }
    if included.order.is_empty() {
        return Err(ProtocolError::new("no commits selected for shallow requests").into());
    }
    Ok(included)
}

/// Breadth-first from `starts` (with their depth), adding commits up to `max` deep.
fn by_depth(
    graph: &mut Graph<'_>,
    included: &mut Included,
    starts: Vec<(ObjectId, u32)>,
    max: u32,
) -> Result<(), ServeError> {
    let mut seen = HashSet::new();
    let mut queue: VecDeque<(ObjectId, u32)> = starts.into();
    while let Some((id, depth)) = queue.pop_front() {
        if !seen.insert(id) {
            continue;
        }
        included.insert(id);
        if depth < max || max >= INFINITE_DEPTH {
            for &parent in graph.parents(id)? {
                queue.push_back((parent, depth + 1));
            }
        }
    }
    Ok(())
}

/// Parent lists and commit times, decoded once per commit.
struct Graph<'repo> {
    repo: &'repo gix::Repository,
    commits: HashMap<ObjectId, (Vec<ObjectId>, i64)>,
}

impl<'repo> Graph<'repo> {
    fn new(repo: &'repo gix::Repository) -> Self {
        Self { repo, commits: HashMap::new() }
    }

    fn is_commit(&self, id: ObjectId) -> bool {
        self.repo.find_header(id).is_ok_and(|header| header.kind() == Kind::Commit)
    }

    fn load(&mut self, id: ObjectId) -> Result<&(Vec<ObjectId>, i64), ServeError> {
        if !self.commits.contains_key(&id) {
            let entry = match self.repo.find_commit(id) {
                Ok(commit) => {
                    let parents = commit.parent_ids().map(|parent| parent.detach()).collect();
                    let time = commit.time().map_err(Error::git)?.seconds;
                    (parents, time)
                }
                // A commit this repository doesn't have (the server itself may be
                // shallow): treat it as having no history.
                Err(_) => (Vec::new(), i64::MIN),
            };
            self.commits.insert(id, entry);
        }
        Ok(&self.commits[&id])
    }

    fn parents(&mut self, id: ObjectId) -> Result<&[ObjectId], ServeError> {
        Ok(&self.load(id)?.0)
    }

    fn time(&mut self, id: ObjectId) -> Result<i64, ServeError> {
        Ok(self.load(id)?.1)
    }
}

pub fn parse_id(hex: &str) -> Result<ObjectId, ProtocolError> {
    ObjectId::from_hex(hex.as_bytes()).map_err(|_| ProtocolError::new(format!("invalid object ID {hex:?}")))
}
