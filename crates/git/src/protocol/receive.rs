//! receive-pack: pushes (`gitprotocol-pack`). Git has no protocol v2 for
//! pushing, so this is v0 (v1 only adds a `version 1` line).
//!
//! A push is a list of ref updates followed by a pack. The pack is indexed into
//! a quarantine directory and checked before anything becomes visible: every
//! object in it must have everything it points at, either in the pack or in the
//! repository. Only then are the pack files moved into the repository and the
//! refs updated, in one transaction for `--atomic` pushes.

use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use gix::bstr::ByteSlice;
use gix::hash::ObjectId;
use gix::objs::Kind;
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog};
use gix::refs::{FullName, Target};

use super::pktline::{self, Packet, ReadError, StreamReader};
use super::sideband::SidebandWriter;
use super::v0::write_advertisement;
use super::{MAX_LINES, ProtocolError, ServeError, answer_error};
use crate::Error;

/// The ref advertisement: `info/refs?service=git-receive-pack`.
pub fn write_receive_pack_advertisement(
    repo: &gix::Repository,
    version_1: bool,
    out: &mut dyn Write,
) -> Result<(), ServeError> {
    let mut lines = Vec::new();
    let platform = repo.references().map_err(Error::git)?;
    for reference in platform.all().map_err(Error::git)? {
        let reference = reference.map_err(Error::Git)?;
        if let Some(id) = reference.target().try_id() {
            lines.push(format!("{id} {}", reference.name().as_bstr()));
        }
    }
    lines.sort_by(|a, b| a[41..].cmp(&b[41..]));
    let capabilities = format!(
        "report-status delete-refs side-band-64k quiet atomic ofs-delta push-options \
         agent=klotho/{} object-format=sha1",
        env!("CARGO_PKG_VERSION")
    );
    write_advertisement(out, version_1, &lines, &capabilities)?;
    Ok(())
}

/// One ref change a push asks for. A null `old` creates the ref, a null `new`
/// deletes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefUpdate {
    pub name: String,
    pub old: ObjectId,
    pub new: ObjectId,
}

impl RefUpdate {
    pub fn is_delete(&self) -> bool {
        self.new.is_null()
    }
}

/// Where Klotho's own checks plug into a push.
pub trait ReceiveHooks {
    /// Called with the updates that passed every check, before any is applied.
    /// An error rejects them all, and its message is shown to the client.
    fn pre_receive(&self, updates: &[RefUpdate], push_options: &[String]) -> Result<(), String>;
    /// Called with the updates that were applied.
    fn post_receive(&self, updates: &[RefUpdate], push_options: &[String]);
}

/// Handles one push: reads the commands and the pack from `input`, writes the
/// report to `out`. `quarantine_root` must be on the same filesystem as the
/// repository, so the checked pack can be moved into it.
pub fn serve_receive_pack(
    repo: &gix::Repository,
    quarantine_root: &Path,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
    interrupt: &AtomicBool,
    hooks: &dyn ReceiveHooks,
) -> Result<(), ServeError> {
    let mut reader = StreamReader::new(input);
    let request = match read_request(&mut reader) {
        Ok(request) => request,
        Err(ReadError::Protocol(err)) => return answer_error(out, err),
        Err(ReadError::Io(err)) => return Err(err.into()),
    };
    if request.updates.is_empty() {
        return Ok(());
    }
    let input = reader.into_inner();

    std::fs::create_dir_all(quarantine_root)?;
    let quarantine = tempfile::Builder::new().prefix("receive-").tempdir_in(quarantine_root)?;
    let mut results: Vec<Result<(), String>> =
        request.updates.iter().map(|update| check_update(repo, update)).collect();
    let mut messages = Vec::new();

    let unpack = if request.updates.iter().all(RefUpdate::is_delete) {
        Ok(None)
    } else {
        receive_pack(repo, input, quarantine.path(), interrupt)
    };
    match &unpack {
        Err(reason) => {
            for result in results.iter_mut() {
                *result = Err("unpacker error".to_owned());
            }
            messages.push(format!("klotho: the pack was rejected: {reason}\n"));
        }
        Ok(received) => {
            if let Err(missing) = check_connectivity(repo, received.as_ref(), &request.updates, interrupt) {
                messages.push(format!("klotho: {missing}\n"));
                for (result, update) in results.iter_mut().zip(&request.updates) {
                    if result.is_ok() && !update.is_delete() {
                        *result = Err("missing necessary objects".to_owned());
                    }
                }
            }
        }
    }

    if request.atomic && results.iter().any(Result::is_err) {
        reject_remaining(&mut results, "atomic push failure");
    }
    let checked = accepted(&request.updates, &results);
    if !checked.is_empty()
        && let Err(reason) = hooks.pre_receive(&checked, &request.push_options)
    {
        messages.push(format!("klotho: {reason}\n"));
        reject_remaining(&mut results, "pre-receive check declined");
    }

    // Make the objects visible, then point the refs at them.
    if results.iter().any(Result::is_ok)
        && let Ok(Some(received)) = &unpack
        && let Err(err) = received.move_into(repo)
    {
        tracing::warn!(%err, "moving a pushed pack into the repository failed");
        reject_remaining(&mut results, "failed to store the pack");
    }
    update_refs(repo, &request.updates, &mut results, request.atomic);

    let applied = accepted(&request.updates, &results);
    let report = Report {
        unpack: unpack.as_ref().map(drop).map_err(Clone::clone),
        updates: &request.updates,
        results: &results,
    };
    let written = write_report(out, &request, &report, &messages);
    if !applied.is_empty() {
        hooks.post_receive(&applied, &request.push_options);
    }
    written
}

struct Request {
    updates: Vec<RefUpdate>,
    push_options: Vec<String>,
    report_status: bool,
    sideband: bool,
    atomic: bool,
}

fn read_request(reader: &mut StreamReader<&mut dyn BufRead>) -> Result<Request, ReadError> {
    let mut request = Request {
        updates: Vec::new(),
        push_options: Vec::new(),
        report_status: false,
        sideband: false,
        atomic: false,
    };
    let mut wants_push_options = false;
    let mut first = true;
    let mut names = HashSet::new();
    loop {
        let line = match reader.next_packet()? {
            Some(Packet::Flush) | None => break,
            Some(packet @ Packet::Data(_)) => packet.as_line()?.to_owned(),
            Some(_) => return Err(ProtocolError::new("unexpected packet in the commands").into()),
        };
        if line.starts_with("shallow ") {
            return Err(ProtocolError::new("pushing from a shallow clone isn't supported").into());
        }
        if line.starts_with("push-cert") {
            return Err(ProtocolError::new("signed pushes aren't supported").into());
        }
        let (command, capabilities) = line.split_once('\0').unwrap_or((&line, ""));
        if first {
            for cap in capabilities.split_ascii_whitespace() {
                match cap {
                    "report-status" => request.report_status = true,
                    "side-band-64k" => request.sideband = true,
                    "atomic" => request.atomic = true,
                    "push-options" => wants_push_options = true,
                    // `quiet` asks for no progress; only errors are ever sent.
                    _ => {}
                }
            }
            first = false;
        }
        if request.updates.len() == MAX_LINES {
            return Err(ProtocolError::new("too many ref updates in one push").into());
        }
        let update = parse_command(command)?;
        if !names.insert(update.name.clone()) {
            return Err(ProtocolError::new(format!("{} is updated twice", update.name)).into());
        }
        request.updates.push(update);
    }
    if wants_push_options {
        loop {
            match reader.next_packet()? {
                Some(Packet::Flush) | None => break,
                Some(packet @ Packet::Data(_)) => {
                    if request.push_options.len() == MAX_LINES {
                        return Err(ProtocolError::new("too many push options").into());
                    }
                    request.push_options.push(packet.as_line()?.to_owned());
                }
                Some(_) => return Err(ProtocolError::new("unexpected packet in the push options").into()),
            }
        }
    }
    Ok(request)
}

fn parse_command(command: &str) -> Result<RefUpdate, ProtocolError> {
    let mut parts = command.splitn(3, ' ');
    let (Some(old), Some(new), Some(name)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(ProtocolError::new(format!("invalid ref update {command:?}")));
    };
    let id = |hex: &str| {
        ObjectId::from_hex(hex.as_bytes())
            .map_err(|_| ProtocolError::new(format!("invalid object ID {hex:?}")))
    };
    Ok(RefUpdate { name: name.to_owned(), old: id(old)?, new: id(new)? })
}

/// Checks one update on its own, before looking at the pack.
fn check_update(repo: &gix::Repository, update: &RefUpdate) -> Result<(), String> {
    let valid_name = update.name.starts_with("refs/") && FullName::try_from(update.name.as_str()).is_ok();
    if !valid_name {
        return Err("funny refname".to_owned());
    }
    if update.old.is_null() && update.new.is_null() {
        return Err("nothing to do".to_owned());
    }
    if update.is_delete()
        && let Ok(head) = repo.head()
        && head.referent_name().is_some_and(|name| name.as_bstr() == update.name.as_bytes().as_bstr())
    {
        return Err("deletion of the current branch prohibited".to_owned());
    }
    Ok(())
}

/// The pack indexed in the quarantine directory.
struct Received {
    bundle: gix_pack::Bundle,
    data_path: PathBuf,
    index_path: PathBuf,
}

impl Received {
    /// Moves the pack into the repository: the data file first, so readers
    /// that find the index always find its data too.
    ///
    /// Both files are flushed to disk first, and the directory after, so an
    /// acknowledged push survives a crash (NFR-STOR-001).
    fn move_into(&self, repo: &gix::Repository) -> std::io::Result<()> {
        let pack_dir = repo.git_dir().join("objects").join("pack");
        std::fs::create_dir_all(&pack_dir)?;
        for path in [&self.data_path, &self.index_path] {
            sync_file(path)?;
        }
        for path in [&self.data_path, &self.index_path] {
            let target = pack_dir.join(path.file_name().expect("pack files have names"));
            // The same name means the same content: the pack is already there.
            if !target.exists() {
                std::fs::rename(path, target)?;
            }
        }
        sync_dir(&pack_dir)
    }
}

/// Indexes the pack from `input` into `quarantine`. Thin packs are completed
/// with base objects from the repository. The error is shown to the client.
fn receive_pack(
    repo: &gix::Repository,
    input: &mut dyn BufRead,
    quarantine: &Path,
    interrupt: &AtomicBool,
) -> Result<Option<Received>, String> {
    let outcome = gix_pack::Bundle::write_to_directory(
        input,
        Some(quarantine),
        &mut gix::progress::Discard,
        interrupt,
        Some(repo.objects.clone()),
        repo.object_hash(),
        gix_pack::bundle::write::Options::default(),
    )
    .map_err(|err| err.to_string())?;
    if let Some(keep) = &outcome.keep_path {
        let _ = std::fs::remove_file(keep);
    }
    // A push of refs to objects the server already has carries an empty pack.
    if outcome.index.num_objects == 0 {
        return Ok(None);
    }
    let (Some(data_path), Some(index_path)) = (outcome.data_path.clone(), outcome.index_path.clone()) else {
        return Err("the pack wasn't written".to_owned());
    };
    let bundle = match outcome.to_bundle() {
        Some(Ok(bundle)) => bundle,
        Some(Err(err)) => return Err(err.to_string()),
        None => return Err("the pack wasn't written".to_owned()),
    };
    Ok(Some(Received { bundle, data_path, index_path }))
}

/// Every object in the pack must have what it points at, in the pack or the
/// repository, and every new ref target must exist. Objects already in the
/// repository are trusted to be complete.
fn check_connectivity(
    repo: &gix::Repository,
    received: Option<&Received>,
    updates: &[RefUpdate],
    interrupt: &AtomicBool,
) -> Result<(), String> {
    let in_pack =
        |id: &gix::hash::oid| received.is_some_and(|received| received.bundle.index.lookup(id).is_some());
    let exists = |id: &gix::hash::oid| in_pack(id) || repo.has_object(id);
    let missing = |id: &gix::hash::oid, from: &ObjectId| format!("object {id} is missing (needed by {from})");

    if let Some(received) = received {
        let bundle = &received.bundle;
        let hash = repo.object_hash();
        let mut buf = Vec::new();
        let mut inflate = gix::zlib::Inflate::default();
        let mut cache = gix_pack::cache::Never;
        for index in 0..bundle.index.num_objects() {
            if interrupt.load(Ordering::Relaxed) {
                return Err("interrupted".to_owned());
            }
            let id = bundle.index.oid_at_index(index).to_owned();
            let (data, _) = bundle
                .get_object_by_index(index, &mut buf, &mut inflate, &mut cache)
                .map_err(|err| format!("object {id} can't be read: {err}"))?;
            let invalid = |err: gix::objs::decode::Error| format!("object {id} is invalid: {err}");
            match data.kind {
                Kind::Commit => {
                    let commit = gix::objs::CommitRef::from_bytes(data.data, hash).map_err(invalid)?;
                    for needed in std::iter::once(commit.tree()).chain(commit.parents()) {
                        if !exists(&needed) {
                            return Err(missing(&needed, &id));
                        }
                    }
                }
                Kind::Tree => {
                    let tree = gix::objs::TreeRef::from_bytes(data.data, hash).map_err(invalid)?;
                    for entry in tree.entries {
                        // Submodule entries point into another repository.
                        if !entry.mode.is_commit() && !exists(entry.oid) {
                            return Err(missing(entry.oid, &id));
                        }
                    }
                }
                Kind::Tag => {
                    let tag = gix::objs::TagRef::from_bytes(data.data, hash).map_err(invalid)?;
                    let target = tag.target();
                    if !exists(&target) {
                        return Err(missing(&target, &id));
                    }
                }
                Kind::Blob => {}
            }
        }
    }

    for update in updates.iter().filter(|update| !update.is_delete()) {
        if !exists(&update.new) {
            return Err(format!("object {} for {} is missing", update.new, update.name));
        }
        if update.name.starts_with("refs/heads/") {
            let kind = match received.and_then(|received| received.bundle.index.lookup(update.new)) {
                Some(_) => kind_in_pack(received.expect("found in it"), &update.new),
                None => repo.find_header(update.new).ok().map(|header| header.kind()),
            };
            if kind != Some(Kind::Commit) {
                return Err(format!("{} must point at a commit, not {}", update.name, update.new));
            }
        }
    }
    Ok(())
}

fn kind_in_pack(received: &Received, id: &ObjectId) -> Option<Kind> {
    let mut buf = Vec::new();
    let mut inflate = gix::zlib::Inflate::default();
    let found = received.bundle.find(id, &mut buf, &mut inflate, &mut gix_pack::cache::Never).ok()??;
    Some(found.0.kind)
}

/// Applies the accepted updates: one transaction for an atomic push, otherwise
/// one per ref so a stale ref doesn't hold up the others.
fn update_refs(
    repo: &gix::Repository,
    updates: &[RefUpdate],
    results: &mut [Result<(), String>],
    atomic: bool,
) {
    let committer =
        gix::actor::SignatureRef { name: "Klotho".into(), email: "klotho@localhost".into(), time: "0 +0000" };
    let pending: Vec<usize> = (0..updates.len()).filter(|&i| results[i].is_ok()).collect();
    if atomic {
        let edits = pending.iter().map(|&i| edit(&updates[i]));
        if let Err(err) = repo.edit_references_as(edits, Some(committer)) {
            tracing::debug!(%err, "atomic ref update failed");
            for &i in &pending {
                results[i] = Err(format!("failed to update ref: {err}"));
            }
        }
    } else {
        for &i in &pending {
            if let Err(err) = repo.edit_references_as([edit(&updates[i])], Some(committer)) {
                results[i] = Err(format!("failed to update ref: {err}"));
            }
        }
    }
    if let Err(err) = sync_refs(repo, updates, results) {
        tracing::warn!(%err, "flushing updated refs to disk failed");
    }
}

/// Flushes the updated loose refs (and `packed-refs`, which deletes rewrite)
/// and their directories to disk. gix's ref transaction renames lock files
/// into place but doesn't fsync them (NFR-STOR-001).
fn sync_refs(
    repo: &gix::Repository,
    updates: &[RefUpdate],
    results: &[Result<(), String>],
) -> std::io::Result<()> {
    let git_dir = repo.git_dir();
    let mut dirs = std::collections::BTreeSet::new();
    for (update, _) in updates.iter().zip(results).filter(|(_, result)| result.is_ok()) {
        let path = git_dir.join(&update.name);
        if !update.is_delete() && path.is_file() {
            sync_file(&path)?;
        }
        if let Some(dir) = path.parent() {
            dirs.insert(dir.to_owned());
        }
    }
    let packed = git_dir.join("packed-refs");
    if packed.is_file() {
        sync_file(&packed)?;
        dirs.insert(git_dir.to_owned());
    }
    for dir in dirs.iter().filter(|dir| dir.is_dir()) {
        sync_dir(dir)?;
    }
    Ok(())
}

/// Flushes a file's contents to disk. Opened for writing because Windows
/// refuses to flush a read-only handle.
fn sync_file(path: &Path) -> std::io::Result<()> {
    std::fs::OpenOptions::new().write(true).open(path)?.sync_all()
}

/// Makes renames and new files in `dir` durable. Windows has no directory
/// fsync; there NTFS journals the rename itself.
fn sync_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    std::fs::File::open(dir)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

fn edit(update: &RefUpdate) -> RefEdit {
    let name = FullName::try_from(update.name.as_str()).expect("checked in check_update");
    let change = if update.is_delete() {
        Change::Delete {
            expected: PreviousValue::MustExistAndMatch(Target::Object(update.old)),
            log: RefLog::AndReference,
        }
    } else {
        Change::Update {
            log: LogChange { mode: RefLog::AndReference, force_create_reflog: false, message: "push".into() },
            expected: if update.old.is_null() {
                PreviousValue::MustNotExist
            } else {
                PreviousValue::MustExistAndMatch(Target::Object(update.old))
            },
            new: Target::Object(update.new),
        }
    };
    RefEdit { change, name, deref: false }
}

fn reject_remaining(results: &mut [Result<(), String>], reason: &str) {
    for result in results.iter_mut().filter(|result| result.is_ok()) {
        *result = Err(reason.to_owned());
    }
}

fn accepted(updates: &[RefUpdate], results: &[Result<(), String>]) -> Vec<RefUpdate> {
    updates
        .iter()
        .zip(results)
        .filter(|(_, result)| result.is_ok())
        .map(|(update, _)| update.clone())
        .collect()
}

struct Report<'a> {
    unpack: Result<(), String>,
    updates: &'a [RefUpdate],
    results: &'a [Result<(), String>],
}

/// The `report-status` answer, on band 1 when the client asked for side-band.
/// Messages go to band 2, where the client prints them as `remote: …`.
fn write_report(
    out: &mut dyn Write,
    request: &Request,
    report: &Report<'_>,
    messages: &[String],
) -> Result<(), ServeError> {
    let mut status = Vec::new();
    if request.report_status {
        match &report.unpack {
            Ok(()) => pktline::write_line(&mut status, "unpack ok")?,
            Err(reason) => pktline::write_line(&mut status, &format!("unpack {}", one_line(reason)))?,
        }
        for (update, result) in report.updates.iter().zip(report.results) {
            match result {
                Ok(()) => pktline::write_line(&mut status, &format!("ok {}", update.name))?,
                Err(reason) => {
                    pktline::write_line(&mut status, &format!("ng {} {}", update.name, one_line(reason)))?
                }
            }
        }
        pktline::write_flush(&mut status)?;
    }
    if !request.sideband {
        out.write_all(&status)?;
        return Ok(());
    }
    let mut sideband = SidebandWriter::new(&mut *out);
    for message in messages {
        // Errors are worth showing even to `--quiet` pushes.
        sideband.progress(message)?;
    }
    if !status.is_empty() {
        sideband.write_all(&status)?;
    }
    pktline::write_flush(sideband.into_inner())?;
    Ok(())
}

fn one_line(text: &str) -> String {
    text.replace(['\n', '\r'], " ")
}
