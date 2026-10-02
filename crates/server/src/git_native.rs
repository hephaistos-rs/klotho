//! Git smart HTTP served by Klotho's own protocol engine (`klotho_git::protocol`,
//! ADR 0004), without the `git` program.
//!
//! So far this covers fetch and clone with protocol v2, which `git` uses by
//! default since 2.26. `git_http` sends everything else (pushes, v0/v1) to
//! the `git` program until the engine covers it too (Phase 1b, steps 2 and 3).

use std::io::{self, BufWriter, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use klotho_core::Core;
use klotho_git::protocol;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tokio_util::task::TaskTracker;

use crate::error::ApiError;
use crate::git_http::{git_response, request_reader};

/// Largest v2 request body accepted. Requests carry `want`/`have` lines, a few
/// MiB even for repositories with very many refs.
const MAX_REQUEST: u64 = 16 * 1024 * 1024;

/// Whether the client asked for protocol v2 (`Git-Protocol: version=2`).
pub fn is_v2(protocol: Option<&str>) -> bool {
    protocol.is_some_and(|value| value.split(':').any(|part| part == "version=2"))
}

/// `GET info/refs?service=git-upload-pack` with protocol v2: the capability list.
pub async fn advertisement(core: &Core, owner: &str, repo: &str) -> Result<Response, ApiError> {
    core.find_repo(owner, repo).await?;
    let mut body = Vec::new();
    protocol::write_v2_advertisement(&mut body)?;
    Ok(git_response("application/x-git-upload-pack-advertisement", Body::from(body)))
}

/// What [`upload_pack`] did with a request.
pub enum UploadPack {
    Served(Response),
    /// The request uses a feature the engine doesn't have yet. The caller hands
    /// these bytes (the request body, already read and gunzipped) to the `git`
    /// program. Interim, see `protocol::needs_git_program`.
    NeedsGit(Vec<u8>),
}

/// `POST git-upload-pack` with protocol v2: one `ls-refs` or `fetch` command.
pub async fn upload_pack(
    core: &Core,
    git_tasks: &TaskTracker,
    owner: &str,
    repo: &str,
    headers: &HeaderMap,
    body: Body,
) -> Result<UploadPack, ApiError> {
    let repo = core.find_repo(owner, repo).await?;

    let mut request = Vec::new();
    request_reader(headers, body).take(MAX_REQUEST + 1).read_to_end(&mut request).await?;
    if request.len() as u64 > MAX_REQUEST {
        return Ok(UploadPack::Served(StatusCode::PAYLOAD_TOO_LARGE.into_response()));
    }
    if protocol::needs_git_program(&request) {
        return Ok(UploadPack::NeedsGit(request));
    }

    // The engine is blocking (gitoxide is synchronous), so it runs on the blocking
    // pool and streams its output back through a channel. Packs can be gigabytes;
    // they're never held in memory (NFR-PERF-004).
    let (tx, rx) = mpsc::channel::<io::Result<Bytes>>(4);
    let store = core.store().clone();
    let id = repo.id;
    git_tasks.spawn_blocking(move || {
        let interrupt = Arc::new(AtomicBool::new(false));
        let writer = ChannelWriter { tx, interrupt: interrupt.clone() };
        let mut out = BufWriter::with_capacity(64 * 1024, writer);
        let result = store
            .open(id)
            .map_err(protocol::ServeError::from)
            .and_then(|repo| protocol::serve_v2(&repo, &request, &mut out, &interrupt));
        let result = result.and_then(|()| out.flush().map_err(Into::into));
        match result {
            Ok(()) => {}
            Err(_) if interrupt.load(Ordering::Relaxed) => {
                tracing::debug!("client went away during upload-pack")
            }
            Err(err) => tracing::warn!(%err, "upload-pack failed"),
        }
    });

    let stream =
        futures_util::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|chunk| (chunk, rx)) });
    Ok(UploadPack::Served(git_response("application/x-git-upload-pack-result", Body::from_stream(stream))))
}

/// Hands written bytes to the async response body. When the client goes away
/// the channel closes, writes fail, and `interrupt` stops pack generation.
struct ChannelWriter {
    tx: mpsc::Sender<io::Result<Bytes>>,
    interrupt: Arc<AtomicBool>,
}

impl Write for ChannelWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.tx.blocking_send(Ok(Bytes::copy_from_slice(buf))).is_err() {
            self.interrupt.store(true, Ordering::Relaxed);
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "client went away"));
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_protocol_v2() {
        assert!(is_v2(Some("version=2")));
        assert!(is_v2(Some("object-format=sha1:version=2")));
        assert!(!is_v2(Some("version=1")));
        assert!(!is_v2(None));
    }
}
