//! Builds the pack a fetch sends, with gitoxide's pack pipeline:
//! commits to send → objects to send (counts) → pack entries → pack bytes.

use std::collections::HashSet;
use std::io::Write;
use std::sync::atomic::AtomicBool;

use gix::hash::ObjectId;
use gix::objs::Kind;
use gix_pack::data::output;
use gix_pack::data::output::count::objects::ObjectExpansion;

use super::select::Selection;
use crate::{Error, Result};

pub struct PackRequest<'a> {
    /// What the client asked for: ref tips, already checked to be advertised.
    pub wants: &'a [ObjectId],
    pub selection: &'a Selection,
    /// Also send annotated tags that point at commits being sent.
    pub include_tags: bool,
}

/// Writes the pack for `request` to `out` (the raw pack bytes, not yet framed).
/// Returns the number of objects written.
pub fn write_pack(
    repo: &gix::Repository,
    request: &PackRequest<'_>,
    out: &mut dyn Write,
    interrupt: &AtomicBool,
) -> Result<u32> {
    let selection = request.selection;
    // Wants that aren't commits (annotated tags), and tags for `include-tag`.
    let mut objects_only: Vec<ObjectId> = Vec::new();
    for want in request.wants {
        if repo.find_header(*want).map_err(Error::git)?.kind() != Kind::Commit {
            objects_only.push(*want);
        }
    }
    if request.include_tags {
        objects_only.extend(tags_pointing_into(repo, &selection.commits)?);
    }
    let (full, additions): (Vec<ObjectId>, Vec<ObjectId>) =
        selection.commits.iter().partition(|id| selection.full_trees.contains(*id));

    // The object cache under gix's in-memory proxy: that's what implements
    // gix-pack's `Find`, and it's shareable across the counting threads.
    let mut db = repo.objects.clone().into_arc().map_err(Error::git)?.into_inner();
    // Counting records where each object sits in which pack, and writing copies
    // from there, so packs must stay mapped for the whole operation.
    db.prevent_pack_unload();

    let mut counts = Vec::new();
    let mut seen = HashSet::new();
    for (inputs, expansion) in [
        (objects_only, ObjectExpansion::AsIs),
        // Each commit contributes the objects its tree adds compared to its
        // parents; root commits contribute their whole tree. Commits the client
        // has aren't inputs, so what they contain isn't sent.
        (additions, ObjectExpansion::TreeAdditionsComparedToAncestor),
        // Commits whose parents the client won't have (a shallow boundary).
        (full, ObjectExpansion::TreeContents),
    ] {
        if inputs.is_empty() {
            continue;
        }
        let (found, _) = output::count::objects(
            db.clone(),
            Box::new(inputs.into_iter().map(Ok)),
            &gix::progress::Discard,
            interrupt,
            output::count::objects::Options {
                thread_limit: None,
                chunk_size: 50,
                input_object_expansion: expansion,
            },
        )
        .map_err(Error::git)?;
        counts.extend(found.into_iter().filter(|count| seen.insert(count.id)));
    }

    let num_entries = u32::try_from(counts.len()).map_err(|_| Error::git(TooManyObjects))?;
    let entries = output::entry::iter_from_counts(
        counts,
        db,
        Box::new(gix::progress::Discard),
        output::entry::iter_from_counts::Options {
            thread_limit: None,
            mode: output::entry::iter_from_counts::Mode::PackCopyAndBaseObjects,
            // Never thin, even when the client allows it: gix would keep any
            // delta whose base isn't in the pack, but the client may not have
            // that base (shallow clones, branches it never fetched). Such
            // deltas are sent as whole objects instead.
            allow_thin_pack: false,
            chunk_size: 10,
            version: gix_pack::data::Version::V2,
            ..Default::default()
        },
    );
    let mut writer = output::bytes::FromEntriesIter::new(
        gix::features::parallel::InOrderIter::from(entries),
        out,
        num_entries,
        gix_pack::data::Version::V2,
        repo.object_hash(),
    );
    for written in writer.by_ref() {
        written.map_err(Error::git)?;
    }
    Ok(num_entries)
}

/// Annotated tags whose target (after peeling) is one of `commits` (`include-tag`).
fn tags_pointing_into(repo: &gix::Repository, commits: &[ObjectId]) -> Result<Vec<ObjectId>> {
    let sending: HashSet<&ObjectId> = commits.iter().collect();
    let mut tags = Vec::new();
    let platform = repo.references().map_err(Error::git)?;
    for reference in platform.tags().map_err(Error::git)? {
        let mut reference = reference.map_err(Error::Git)?;
        let Some(id) = reference.target().try_id().map(ToOwned::to_owned) else { continue };
        if repo.find_header(id).map_err(Error::git)?.kind() != Kind::Tag {
            continue;
        }
        let peeled = reference.peel_to_id().map_err(Error::git)?;
        if sending.contains(&peeled.detach()) {
            tags.push(id);
        }
    }
    Ok(tags)
}

#[derive(Debug, thiserror::Error)]
#[error("too many objects for one pack")]
struct TooManyObjects;
