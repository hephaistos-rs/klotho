//! Git protocol version 2 for upload-pack: the capability advertisement and the
//! `ls-refs` and `fetch` commands (`gitprotocol-v2`).
//!
//! Over HTTP every request is complete in itself, so each call handles one
//! command from a fully read request body.

use std::collections::HashSet;
use std::io::{BufWriter, Write};
use std::sync::atomic::AtomicBool;

use gix::hash::ObjectId;
use gix::objs::Kind;

use super::pack::{PackRequest, write_pack};
use super::pktline::{self, Packet, Reader};
use super::sideband::SidebandWriter;
use super::{ProtocolError, ServeError};
use crate::Error;

/// Most `want` and `have` lines a single request may carry. Git sends haves in
/// rounds of at most a few hundred, and wants are bounded by the number of refs.
const MAX_LINES: usize = 100_000;

/// What `git` sees when it asks which protocol features we have.
pub fn write_advertisement(out: &mut impl Write) -> std::io::Result<()> {
    pktline::write_line(out, "version 2")?;
    pktline::write_line(out, &format!("agent=klotho/{}", env!("CARGO_PKG_VERSION")))?;
    pktline::write_line(out, "ls-refs=unborn")?;
    // Shallow fetches are advertised, as `git upload-pack` does, but the caller
    // hands them to the `git` program until this engine supports them (Phase 1b,
    // step 4; see `needs_git_program`). Filters are off, as in git's default.
    pktline::write_line(out, "fetch=shallow")?;
    pktline::write_line(out, "server-option")?;
    pktline::write_line(out, "object-format=sha1")?;
    pktline::write_flush(out)
}

/// Handles one v2 command from `request`, writing the response to `out`.
///
/// Problems with the request are answered with an `ERR` line the client
/// prints. An error is only returned when nothing useful can be sent any more,
/// e.g. the connection is gone.
pub fn serve(
    repo: &gix::Repository,
    request: &[u8],
    out: &mut dyn Write,
    interrupt: &AtomicBool,
) -> Result<(), ServeError> {
    if repo.object_hash() != gix::hash::Kind::Sha1 {
        return answer_error(out, ProtocolError::new("only SHA-1 repositories are supported"));
    }
    let mut reader = Reader::new(request);
    let command = match read_command(&mut reader) {
        Ok(command) => command,
        Err(err) => return answer_error(out, err),
    };
    let result = match command.as_str() {
        "ls-refs" => ls_refs(repo, &mut reader, out),
        "fetch" => fetch(repo, &mut reader, out, interrupt),
        other => Err(ServeError::Protocol(ProtocolError::new(format!("unknown command {other:?}")))),
    };
    match result {
        Err(ServeError::Protocol(err)) => answer_error(out, err),
        other => other,
    }
}

/// Whether `request` uses a feature this engine doesn't implement yet, so the
/// caller must hand it to the `git` program instead: shallow fetches
/// (`shallow`, `deepen…`). Interim, until Phase 1b step 4.
pub fn needs_git_program(request: &[u8]) -> bool {
    let mut reader = Reader::new(request);
    while let Ok(Some(packet)) = reader.next_packet() {
        if let Ok(line) = packet.as_line()
            && (line.starts_with("shallow ") || line.starts_with("deepen"))
        {
            return true;
        }
    }
    false
}

fn answer_error(out: &mut dyn Write, err: ProtocolError) -> Result<(), ServeError> {
    pktline::write_line(out, &format!("ERR {err}"))?;
    pktline::write_flush(out)?;
    Ok(())
}

/// The capability section: `command=…`, then capabilities, up to a delimiter.
fn read_command(reader: &mut Reader<'_>) -> Result<String, ProtocolError> {
    let mut command = None;
    loop {
        match reader.next_packet()? {
            Some(Packet::Delim) => break,
            Some(Packet::Flush) | None if command.is_some() => break,
            Some(packet @ Packet::Data(_)) => {
                let line = packet.as_line()?;
                if let Some(name) = line.strip_prefix("command=") {
                    command = Some(name.to_owned());
                } else if let Some(format) = line.strip_prefix("object-format=")
                    && format != "sha1"
                {
                    return Err(ProtocolError::new(format!("unsupported object format {format:?}")));
                }
                // agent=…, server-option=… and session-id=… need no action.
            }
            _ => return Err(ProtocolError::new("expected a command")),
        }
    }
    command.ok_or_else(|| ProtocolError::new("no command given"))
}

/// The argument lines after the delimiter, up to the flush.
fn read_args<'a>(reader: &mut Reader<'a>) -> Result<Vec<&'a str>, ProtocolError> {
    let mut args = Vec::new();
    loop {
        match reader.next_packet()? {
            Some(Packet::Flush) | None => return Ok(args),
            Some(packet @ Packet::Data(_)) => {
                if args.len() == MAX_LINES {
                    return Err(ProtocolError::new("too many lines in one request"));
                }
                args.push(packet.as_line()?);
            }
            Some(_) => return Err(ProtocolError::new("unexpected packet in the arguments")),
        }
    }
}

/// One ref as `ls-refs` reports it.
struct RefLine {
    name: String,
    /// `None` for an unborn HEAD.
    id: Option<ObjectId>,
    symref_target: Option<String>,
    /// The object an annotated tag points at, after peeling.
    peeled: Option<ObjectId>,
}

/// Every ref, with HEAD first. These are also the only objects `fetch` accepts
/// as wants, so nothing unadvertised can be fetched by ID.
fn advertised_refs(repo: &gix::Repository) -> Result<Vec<RefLine>, Error> {
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

fn ls_refs(repo: &gix::Repository, reader: &mut Reader<'_>, out: &mut dyn Write) -> Result<(), ServeError> {
    let (mut symrefs, mut peel, mut unborn, mut prefixes) = (false, false, false, Vec::new());
    for arg in read_args(reader)? {
        match arg {
            "symrefs" => symrefs = true,
            "peel" => peel = true,
            "unborn" => unborn = true,
            _ => match arg.strip_prefix("ref-prefix ") {
                Some(prefix) => prefixes.push(prefix),
                None => return Err(ProtocolError::new(format!("unknown ls-refs argument {arg:?}")).into()),
            },
        }
    }

    for line in advertised_refs(repo)? {
        if !prefixes.is_empty() && !prefixes.iter().any(|prefix| line.name.starts_with(prefix)) {
            continue;
        }
        let mut text = match line.id {
            Some(id) => format!("{id} {}", line.name),
            None if unborn => format!("unborn {}", line.name),
            None => continue,
        };
        if symrefs && let Some(target) = &line.symref_target {
            text.push_str(&format!(" symref-target:{target}"));
        }
        if peel && let Some(peeled) = line.peeled {
            text.push_str(&format!(" peeled:{peeled}"));
        }
        pktline::write_line(out, &text)?;
    }
    pktline::write_flush(out)?;
    Ok(())
}

fn fetch(
    repo: &gix::Repository,
    reader: &mut Reader<'_>,
    out: &mut dyn Write,
    interrupt: &AtomicBool,
) -> Result<(), ServeError> {
    let (mut wants, mut haves) = (Vec::new(), Vec::new());
    let (mut done, mut thin, mut no_progress, mut include_tags) = (false, false, false, false);
    for arg in read_args(reader)? {
        match arg {
            "done" => done = true,
            "thin-pack" => thin = true,
            "no-progress" => no_progress = true,
            "include-tag" => include_tags = true,
            "ofs-delta" => {}
            _ => {
                if let Some(id) = arg.strip_prefix("want ") {
                    wants.push(parse_id(id)?);
                } else if let Some(id) = arg.strip_prefix("have ") {
                    haves.push(parse_id(id)?);
                } else if arg.starts_with("shallow ") || arg.starts_with("deepen") {
                    return Err(ProtocolError::new("shallow clones aren't supported yet").into());
                } else if arg.starts_with("filter ") {
                    return Err(ProtocolError::new("partial clones (--filter) aren't supported yet").into());
                } else {
                    return Err(ProtocolError::new(format!("unknown fetch argument {arg:?}")).into());
                }
            }
        }
    }
    if wants.is_empty() {
        return Err(ProtocolError::new("fetch without any want").into());
    }

    let advertised: HashSet<ObjectId> =
        advertised_refs(repo)?.into_iter().flat_map(|line| line.id.into_iter().chain(line.peeled)).collect();
    if let Some(unknown) = wants.iter().find(|want| !advertised.contains(*want)) {
        return Err(ProtocolError::new(format!("upload-pack: not our ref {unknown}")).into());
    }

    // Haves that are commits we have too. Anything else is ignored.
    let mut common = Vec::new();
    for have in haves {
        if repo.find_header(have).is_ok_and(|header| header.kind() == Kind::Commit) {
            common.push(have);
        }
    }

    if !done {
        pktline::write_line(out, "acknowledgments")?;
        if common.is_empty() {
            pktline::write_line(out, "NAK")?;
            pktline::write_flush(out)?;
            return Ok(());
        }
        for id in &common {
            pktline::write_line(out, &format!("ACK {id}"))?;
        }
        // One common commit is enough for a correct pack: everything reachable
        // from it is left out. The pack may contain objects the client has
        // through other common commits we didn't wait for; that costs some
        // bandwidth, not correctness.
        pktline::write_line(out, "ready")?;
        pktline::write_delim(out)?;
    }

    pktline::write_line(out, "packfile")?;
    let request = PackRequest { wants: &wants, common: &common, thin, include_tags };
    let mut sideband = BufWriter::with_capacity(pktline::MAX_DATA - 1, SidebandWriter::new(&mut *out));
    let result = write_pack(repo, &request, &mut sideband, interrupt);
    let mut sideband = sideband.into_inner().map_err(|err| err.into_error())?;
    match result {
        Ok(objects) => {
            if !no_progress {
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

fn parse_id(hex: &str) -> Result<ObjectId, ProtocolError> {
    ObjectId::from_hex(hex.as_bytes()).map_err(|_| ProtocolError::new(format!("invalid object ID {hex:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(lines: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        pktline::write_line(&mut out, "command=fetch").unwrap();
        pktline::write_delim(&mut out).unwrap();
        for line in lines {
            pktline::write_line(&mut out, line).unwrap();
        }
        pktline::write_flush(&mut out).unwrap();
        out
    }

    #[test]
    fn shallow_requests_go_to_the_git_program() {
        let want = "want 0123456789012345678901234567890123456789";
        assert!(needs_git_program(&request(&[want, "deepen 1"])));
        assert!(needs_git_program(&request(&[want, "deepen-since 1700000000"])));
        assert!(needs_git_program(&request(&[want, "shallow 0123456789012345678901234567890123456789"])));
        assert!(!needs_git_program(&request(&[want, "done"])));
    }

    #[test]
    fn advertises_what_git_clients_look_for() {
        let mut out = Vec::new();
        write_advertisement(&mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        for capability in ["version 2\n", "ls-refs=unborn\n", "fetch=shallow\n", "object-format=sha1\n"] {
            assert!(text.contains(capability), "{capability:?} missing from {text:?}");
        }
        assert!(text.ends_with("0000"));
    }
}
