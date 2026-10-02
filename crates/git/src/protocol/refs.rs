//! The refs a repository shows to clients, shared by every protocol version.

use std::collections::HashSet;

use gix::hash::ObjectId;
use gix::objs::Kind;

use crate::Error;

/// One ref as the advertisement or `ls-refs` reports it.
pub struct RefLine {
    pub name: String,
    /// `None` for an unborn HEAD.
    pub id: Option<ObjectId>,
    pub symref_target: Option<String>,
    /// The object an annotated tag points at, after peeling.
    pub peeled: Option<ObjectId>,
}

/// Every ref, with HEAD first. These are also the only objects a fetch accepts
/// as wants, so nothing unadvertised can be fetched by ID.
pub fn advertised_refs(repo: &gix::Repository) -> Result<Vec<RefLine>, Error> {
    let mut lines = Vec::new();
    let head = repo.head().map_err(Error::git)?;
    let head_target = head.referent_name().map(|name| name.as_bstr().to_string());
    let head_id = head.id().map(|id| id.detach());
    lines.push(RefLine { name: "HEAD".to_owned(), id: head_id, symref_target: head_target, peeled: None });

    let platform = repo.references().map_err(Error::git)?;
    let mut refs = Vec::new();
    for reference in platform.all().map_err(Error::git)? {
        let mut reference = reference.map_err(Error::Git)?;
        let name = reference.name().as_bstr().to_string();
        let symref_target = reference.target().try_name().map(|target| target.as_bstr().to_string());
        // Read the direct target before anything else: gix peels references in place.
        let id = match reference.target().try_id() {
            Some(id) => id.to_owned(),
            None => match reference.peel_to_id() {
                Ok(id) => id.detach(),
                Err(_) => continue, // a symbolic ref to a missing ref
            },
        };
        let Ok(object) = repo.find_object(id) else {
            continue; // a ref to a missing object
        };
        let peeled = if object.kind == Kind::Tag {
            Some(object.peel_tags_to_end().map_err(Error::git)?.id)
        } else {
            None
        };
        refs.push(RefLine { name, id: Some(id), symref_target, peeled });
    }
    refs.sort_by(|a, b| a.name.cmp(&b.name));
    lines.extend(refs);
    Ok(lines)
}

/// The objects a client may ask for: ref tips and what annotated tags peel to.
pub fn wantable(lines: &[RefLine]) -> HashSet<ObjectId> {
    lines.iter().flat_map(|line| line.id.into_iter().chain(line.peeled)).collect()
}
