//! The parts of a fetch that don't depend on the protocol version.

use std::io::{BufWriter, Write};
use std::sync::atomic::AtomicBool;

use gix::hash::ObjectId;
use gix::objs::Kind;

use super::pack::{PackRequest, write_pack};
use super::pktline;
use super::refs::{advertised_refs, wantable};
use super::select::Selection;
use super::sideband::SidebandWriter;
use super::{ProtocolError, ServeError};

/// Refuses wants that aren't advertised ref tips (or what tags peel to), so
/// unreachable objects can't be fetched by ID.
pub fn check_wants(repo: &gix::Repository, wants: &[ObjectId]) -> Result<(), ServeError> {
    let allowed = wantable(&advertised_refs(repo)?);
    match wants.iter().find(|want| !allowed.contains(*want)) {
        Some(unknown) => Err(ProtocolError::new(format!("upload-pack: not our ref {unknown}")).into()),
        None => Ok(()),
    }
}

/// The haves that are commits we have too. Anything else is ignored.
pub fn common_commits(repo: &gix::Repository, haves: &[ObjectId]) -> Vec<ObjectId> {
    haves
        .iter()
        .copied()
        .filter(|have| repo.find_header(*have).is_ok_and(|header| header.kind() == Kind::Commit))
        .collect()
}

/// The `shallow`/`unshallow` lines announcing the client's new shallow boundary.
pub fn write_shallow_lines(out: &mut dyn Write, selection: &Selection) -> std::io::Result<()> {
    for id in &selection.shallow {
        pktline::write_line(out, &format!("shallow {id}"))?;
    }
    for id in &selection.unshallow {
        pktline::write_line(out, &format!("unshallow {id}"))?;
    }
    Ok(())
}

pub struct PackOptions {
    pub include_tags: bool,
    pub progress: bool,
    /// Multiplex with `side-band-64k`. Without it the pack goes out as raw bytes,
    /// with no room for progress or error messages.
    pub sideband: bool,
}

/// Sends the pack. Failures after the pack started can only be reported on the
/// side-band; without one the client sees a truncated pack.
pub fn send_pack(
    repo: &gix::Repository,
    wants: &[ObjectId],
    selection: &Selection,
    options: &PackOptions,
    out: &mut dyn Write,
    interrupt: &AtomicBool,
) -> Result<(), ServeError> {
    let request = PackRequest { wants, selection, include_tags: options.include_tags };
    if !options.sideband {
        let mut out = BufWriter::with_capacity(64 * 1024, out);
        let result = write_pack(repo, &request, &mut out, interrupt);
        out.flush()?;
        return result.map(drop).map_err(Into::into);
    }
    let mut sideband = BufWriter::with_capacity(pktline::MAX_DATA - 1, SidebandWriter::new(&mut *out));
    let result = write_pack(repo, &request, &mut sideband, interrupt);
    let mut sideband = sideband.into_inner().map_err(|err| err.into_error())?;
    match result {
        Ok(objects) => {
            if options.progress {
                sideband.progress(&format!("Klotho: sent {objects} objects\n"))?;
            }
        }
        Err(err) => {
            tracing::warn!(%err, "building a pack failed");
            sideband.error("klotho: failed to build the pack; see the server log\n")?;
        }
    }
    pktline::write_flush(sideband.into_inner())?;
    Ok(())
}
