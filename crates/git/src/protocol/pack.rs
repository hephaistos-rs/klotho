//! Builds the pack a fetch sends, with gitoxide's pack pipeline:
//! commits to send → objects to send (counts) → pack entries → pack bytes.

use std::io::Write;
use std::sync::atomic::AtomicBool;

use gix::hash::ObjectId;
use gix::objs::Kind;
use gix_pack::data::output;

use crate::{Error, Result};

pub struct PackRequest<'a> {
    /// What the client asked for: ref tips, already checked to be advertised.
    pub wants: &'a [ObjectId],
    /// Commits the client has and we have too. Nothing reachable from them is sent.
    pub common: &'a [ObjectId],
    /// The client accepts deltas against objects it has but the pack doesn't contain.
    pub thin: bool,
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
    let commits = commits_to_send(repo, request)?;
    let mut inputs: Vec<ObjectId> = request.wants.to_vec();
    if request.include_tags {
        inputs.extend(tags_pointing_into(repo, &commits)?);
    }
    inputs.extend(commits.iter().copied());

    // The object cache under gix's in-memory proxy: that's what implements
    // gix-pack's `Find`, and it's shareable across the counting threads.
    let mut db = repo.objects.clone().into_arc().map_err(Error::git)?.into_inner();
    // Counting records where each object sits in which pack, and writing copies
    // from there, so packs must stay mapped for the whole operation.
    db.prevent_pack_unload();
    let (counts, _) = output::count::objects(
        db.clone(),
        Box::new(inputs.into_iter().map(Ok)),
        &gix::progress::Discard,
        interrupt,
        output::count::objects::Options {
            thread_limit: None,
            chunk_size: 50,
            // Each commit contributes the objects its tree adds compared to its
            // parents; root commits contribute their whole tree. Commits the
            // client has aren't inputs, so what they contain isn't sent.
            input_object_expansion: output::count::objects::ObjectExpansion::TreeAdditionsComparedToAncestor,
        },
    )
    .map_err(Error::git)?;

    let num_entries = u32::try_from(counts.len()).map_err(|_| Error::git(TooManyObjects))?;
    let entries = output::entry::iter_from_counts(
        counts,
        db,
        Box::new(gix::progress::Discard),
        output::entry::iter_from_counts::Options {
            thread_limit: None,
            mode: output::entry::iter_from_counts::Mode::PackCopyAndBaseObjects,
            allow_thin_pack: request.thin,
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

/// The commits reachable from the wants but not from what the client has.
fn commits_to_send(repo: &gix::Repository, request: &PackRequest<'_>) -> Result<Vec<ObjectId>> {
    let mut tips = Vec::new();
    for want in request.wants {
        let object = repo.find_object(*want).map_err(Error::git)?;
        if let Ok(commit) = object.peel_to_kind(Kind::Commit) {
            tips.push(commit.id);
        }
    }
    if tips.is_empty() {
        return Ok(Vec::new());
    }
    let walk = repo.rev_walk(tips).with_hidden(request.common.iter().copied()).all().map_err(Error::git)?;
    walk.map(|info| Ok(info.map_err(Error::git)?.id)).collect()
}

/// Annotated tags whose target (after peeling) is one of `commits` (`include-tag`).
fn tags_pointing_into(repo: &gix::Repository, commits: &[ObjectId]) -> Result<Vec<ObjectId>> {
    let sending: std::collections::HashSet<&ObjectId> = commits.iter().collect();
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
