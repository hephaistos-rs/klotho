//! Git protocol version 2 for upload-pack: the capability advertisement and the
//! `ls-refs` and `fetch` commands (`gitprotocol-v2`).
//!
//! Over HTTP every request is complete in itself, so each call handles one
//! command from a fully read request body.

use std::io::Write;
use std::sync::atomic::AtomicBool;

use gix::hash::ObjectId;

use super::fetch::{PackOptions, check_wants, common_commits, send_pack, write_shallow_lines};
use super::pktline::{self, Packet, Reader};
use super::refs::advertised_refs;
use super::select::{ShallowRequest, parse_id, select};
use super::{MAX_LINES, ProtocolError, ServeError, answer_error};

/// What `git` sees when it asks which protocol features we have.
pub fn write_advertisement(out: &mut impl Write) -> std::io::Result<()> {
    pktline::write_line(out, "version 2")?;
    pktline::write_line(out, &format!("agent=klotho/{}", env!("CARGO_PKG_VERSION")))?;
    pktline::write_line(out, "ls-refs=unborn")?;
    // Partial clone filters are off, as in git's default.
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
    let (mut wants, mut haves) = (Vec::<ObjectId>::new(), Vec::new());
    let mut shallow = ShallowRequest::default();
    let (mut done, mut no_progress, mut include_tags) = (false, false, false);
    for arg in read_args(reader)? {
        match arg {
            "done" => done = true,
            // Packs are never thin; see `pack.rs`.
            "thin-pack" => {}
            "no-progress" => no_progress = true,
            "include-tag" => include_tags = true,
            "ofs-delta" => {}
            _ => {
                if let Some(id) = arg.strip_prefix("want ") {
                    wants.push(parse_id(id)?);
                } else if let Some(id) = arg.strip_prefix("have ") {
                    haves.push(parse_id(id)?);
                } else if arg.starts_with("filter ") {
                    return Err(ProtocolError::new("partial clones (--filter) aren't supported").into());
                } else if !shallow.parse_line(arg)? {
                    return Err(ProtocolError::new(format!("unknown fetch argument {arg:?}")).into());
                }
            }
        }
    }
    if wants.is_empty() {
        return Err(ProtocolError::new("fetch without any want").into());
    }
    check_wants(repo, &wants)?;
    let common = common_commits(repo, &haves);

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

    let selection = select(repo, &wants, &common, &shallow)?;
    if shallow.deepens() {
        pktline::write_line(out, "shallow-info")?;
        write_shallow_lines(out, &selection)?;
        pktline::write_delim(out)?;
    }
    pktline::write_line(out, "packfile")?;
    let options = PackOptions { include_tags, progress: !no_progress, sideband: true };
    send_pack(repo, &wants, &selection, &options, out, interrupt)
}

#[cfg(test)]
mod tests {
    use super::*;

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
