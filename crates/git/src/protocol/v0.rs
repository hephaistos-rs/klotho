//! Git protocol versions 0 and 1 for upload-pack (`gitprotocol-pack`), for
//! clients older than git 2.26 or set to `protocol.version=0`. Version 1 is
//! version 0 with a `version 1` line in front of the advertisement.
//!
//! Over HTTP the negotiation is stateless: each request repeats the wants, and
//! the haves the server acknowledged so far, so every call handles one
//! complete request body as if the conversation had continued.

use std::io::Write;
use std::sync::atomic::AtomicBool;

use gix::hash::ObjectId;

use super::fetch::{PackOptions, check_wants, common_commits, send_pack, write_shallow_lines};
use super::pktline::{self, Packet, Reader};
use super::refs::advertised_refs;
use super::select::{ShallowRequest, parse_id, select};
use super::{MAX_LINES, ProtocolError, ServeError, answer_error};

const NULL_ID: &str = "0000000000000000000000000000000000000000";

/// The ref advertisement: `info/refs?service=git-upload-pack` without v2.
pub fn write_upload_pack_advertisement(
    repo: &gix::Repository,
    version_1: bool,
    out: &mut dyn Write,
) -> Result<(), ServeError> {
    let refs = advertised_refs(repo)?;
    let mut capabilities = String::from(
        "multi_ack_detailed no-done side-band-64k ofs-delta shallow deepen-since deepen-not \
         deepen-relative no-progress include-tag",
    );
    if let Some(head) = refs.first().filter(|head| head.id.is_some())
        && let Some(target) = &head.symref_target
    {
        capabilities.push_str(&format!(" symref=HEAD:{target}"));
    }
    capabilities.push_str(&format!(" agent=klotho/{} object-format=sha1", env!("CARGO_PKG_VERSION")));

    let mut lines = Vec::new();
    for line in &refs {
        let Some(id) = line.id else { continue }; // an unborn HEAD isn't listed
        lines.push(format!("{id} {}", line.name));
        if let Some(peeled) = line.peeled {
            lines.push(format!("{peeled} {}^{{}}", line.name));
        }
    }
    write_advertisement(out, version_1, &lines, &capabilities)?;
    Ok(())
}

/// Writes `lines` with `capabilities` after the first one, as v0 does.
pub fn write_advertisement(
    out: &mut dyn Write,
    version_1: bool,
    lines: &[String],
    capabilities: &str,
) -> std::io::Result<()> {
    if version_1 {
        pktline::write_line(out, "version 1")?;
    }
    match lines.split_first() {
        Some((first, rest)) => {
            pktline::write_line(out, &format!("{first}\0{capabilities}"))?;
            for line in rest {
                pktline::write_line(out, line)?;
            }
        }
        // An empty repository still has to announce its capabilities.
        None => pktline::write_line(out, &format!("{NULL_ID} capabilities^{{}}\0{capabilities}"))?,
    }
    pktline::write_flush(out)
}

/// Handles one upload-pack request, writing the response to `out`.
pub fn serve_upload_pack(
    repo: &gix::Repository,
    request: &[u8],
    out: &mut dyn Write,
    interrupt: &AtomicBool,
) -> Result<(), ServeError> {
    match upload_pack(repo, &mut Reader::new(request), out, interrupt) {
        Err(ServeError::Protocol(err)) => answer_error(out, err),
        other => other,
    }
}

#[derive(Default)]
struct Capabilities {
    multi_ack_detailed: bool,
    no_done: bool,
    sideband: bool,
    no_progress: bool,
    include_tags: bool,
}

fn upload_pack(
    repo: &gix::Repository,
    reader: &mut Reader<'_>,
    out: &mut dyn Write,
    interrupt: &AtomicBool,
) -> Result<(), ServeError> {
    // The wants, with the capabilities on the first one, and the shallow lines.
    let (mut wants, mut shallow, mut caps) = (Vec::<ObjectId>::new(), ShallowRequest::default(), None);
    loop {
        let line = match reader.next_packet()? {
            Some(Packet::Flush) => break,
            // The client has nothing to fetch.
            None if wants.is_empty() => return Ok(()),
            Some(packet @ Packet::Data(_)) => packet.as_line()?,
            _ => return Err(ProtocolError::new("unexpected packet in the wants").into()),
        };
        if wants.len() + shallow.client.len() == MAX_LINES {
            return Err(ProtocolError::new("too many lines in one request").into());
        }
        if let Some(rest) = line.strip_prefix("want ") {
            let (id, rest) = rest.split_once(' ').unwrap_or((rest, ""));
            wants.push(parse_id(id)?);
            if caps.is_none() {
                caps = Some(parse_capabilities(rest, &mut shallow));
            }
        } else if line.starts_with("filter ") {
            return Err(ProtocolError::new("partial clones (--filter) aren't supported").into());
        } else if !shallow.parse_line(line)? {
            return Err(ProtocolError::new(format!("unexpected line {line:?}")).into());
        }
    }
    let caps = caps.unwrap_or_default();
    if wants.is_empty() {
        return Ok(());
    }
    check_wants(repo, &wants)?;
    if shallow.deepens() {
        // The new boundary goes out with every round. It doesn't depend on the
        // haves; what's sent does, so the pack's selection is made again below.
        write_shallow_lines(out, &select(repo, &wants, &[], &shallow)?)?;
        pktline::write_flush(out)?;
    }

    // The haves, ending in a flush (another round follows) or `done`.
    let (mut common, mut last, mut ready, mut done) = (Vec::new(), None, false, false);
    loop {
        match reader.next_packet()? {
            // Only the wants were sent: a shallow clone asks for its boundary first.
            None => return Ok(()),
            Some(Packet::Flush) => {
                if caps.multi_ack_detailed
                    && let Some(last) = last.filter(|_| !ready)
                {
                    // One common commit is enough for a correct pack, as in v2.
                    pktline::write_line(out, &format!("ACK {last} ready"))?;
                    ready = true;
                }
                if common.is_empty() || caps.multi_ack_detailed {
                    pktline::write_line(out, "NAK")?;
                }
                if caps.no_done && ready {
                    if let Some(last) = last {
                        pktline::write_line(out, &format!("ACK {last}"))?;
                    }
                    break;
                }
                // The client sends the next round as a new request.
                if reader.is_empty() {
                    return Ok(());
                }
            }
            Some(packet @ Packet::Data(_)) => {
                let line = packet.as_line()?;
                if line == "done" {
                    done = true;
                    break;
                }
                let id = line
                    .strip_prefix("have ")
                    .ok_or_else(|| ProtocolError::new(format!("unexpected line {line:?}")))?;
                let id = parse_id(id)?;
                if common.len() == MAX_LINES {
                    return Err(ProtocolError::new("too many lines in one request").into());
                }
                if !common_commits(repo, &[id]).is_empty() {
                    common.push(id);
                    last = Some(id);
                    if caps.multi_ack_detailed {
                        pktline::write_line(out, &format!("ACK {id} common"))?;
                    } else if common.len() == 1 {
                        pktline::write_line(out, &format!("ACK {id}"))?;
                    }
                }
            }
            Some(_) => return Err(ProtocolError::new("unexpected packet in the haves").into()),
        }
    }
    if done {
        match last {
            Some(last) if caps.multi_ack_detailed => pktline::write_line(out, &format!("ACK {last}"))?,
            Some(_) => {} // the single ACK went out already
            None => pktline::write_line(out, "NAK")?,
        }
    }

    let selection = select(repo, &wants, &common, &shallow)?;
    let options =
        PackOptions { include_tags: caps.include_tags, progress: !caps.no_progress, sideband: caps.sideband };
    send_pack(repo, &wants, &selection, &options, out, interrupt)
}

fn parse_capabilities(text: &str, shallow: &mut ShallowRequest) -> Capabilities {
    let mut caps = Capabilities::default();
    for cap in text.split_ascii_whitespace() {
        match cap {
            "multi_ack_detailed" => caps.multi_ack_detailed = true,
            "no-done" => caps.no_done = true,
            "side-band-64k" => caps.sideband = true,
            // Packs are never thin; see `pack.rs`.
            "thin-pack" => {}
            "no-progress" => caps.no_progress = true,
            "include-tag" => caps.include_tags = true,
            "deepen-relative" => shallow.relative = true,
            // ofs-delta, shallow, deepen-since, deepen-not, agent=…, object-format=…
            _ => {}
        }
    }
    caps
}
