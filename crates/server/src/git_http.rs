//! Git's "smart HTTP" protocol, so `git clone/fetch/push http://host/<owner>/<repo>.git`
//! work. `.git` is optional and names are case-insensitive (FR-NAME-021, 040).
//!
//! Everything git-specific happens in `klotho_git::protocol`, in-process with
//! gitoxide (ADR 0004). This module only moves bytes between HTTP and that
//! engine. The engine is blocking, so it runs on the blocking pool; fetch
//! responses stream out as they're produced, and pushes stream in.
//!
//! See <https://git-scm.com/docs/http-protocol>.
//!
//! Every request is authorized with `klotho-core`'s `repo_for`: reading for
//! clone and fetch, writing for push (FR-ACL-050). Anonymous requests that need
//! more get a `401` challenge so git asks for a token (FR-AUTH-023).

use std::io::{self, BufReader, BufWriter, Write};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_compression::tokio::bufread::GzipDecoder;
use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use futures_util::TryStreamExt;
use klotho_core::{Action, Actor, Core, Error, Repo};
use klotho_git::protocol::{self, ReceiveHooks, RefUpdate};
use serde::Deserialize;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::mpsc;
use tokio_util::io::{StreamReader, SyncIoBridge};
use tokio_util::task::TaskTracker;

use crate::AppState;
use crate::auth::{challenge, git_actor};
use crate::error::ApiError;

/// Largest fetch request body accepted. Requests carry `want`/`have` lines, a
/// few MiB even for repositories with very many refs. Pushes have no limit here.
const MAX_FETCH_REQUEST: u64 = 16 * 1024 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{owner}/{repo}/info/refs", get(info_refs))
        .route("/{owner}/{repo}/git-upload-pack", post(upload_pack))
        .route("/{owner}/{repo}/git-receive-pack", post(receive_pack))
        // Pushes can be arbitrarily large.
        .layer(DefaultBodyLimit::disable())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Service {
    UploadPack,
    ReceivePack,
}

impl Service {
    fn name(self) -> &'static str {
        match self {
            Self::UploadPack => "upload-pack",
            Self::ReceivePack => "receive-pack",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Version {
    V0,
    V1,
    V2,
}

/// The protocol version the client asked for in its `Git-Protocol` header
/// (`version=2`, possibly among other `key=value` parts separated by `:`).
fn protocol_version(headers: &HeaderMap) -> Version {
    let Some(value) = headers.get("git-protocol").and_then(|value| value.to_str().ok()) else {
        return Version::V0;
    };
    let parts: Vec<&str> = value.split(':').collect();
    if parts.contains(&"version=2") {
        Version::V2
    } else if parts.contains(&"version=1") {
        Version::V1
    } else {
        Version::V0
    }
}

#[derive(Deserialize)]
struct InfoRefsQuery {
    service: Option<String>,
}

/// First request of every clone/fetch/push: lists the repo's refs and capabilities.
async fn info_refs(
    State(core): State<Core>,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<InfoRefsQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let service = match query.service.as_deref() {
        Some("git-upload-pack") => Service::UploadPack,
        Some("git-receive-pack") => Service::ReceivePack,
        _ => return Ok((StatusCode::FORBIDDEN, "only the smart HTTP protocol is supported").into_response()),
    };
    let action = if service == Service::ReceivePack { Action::Write } else { Action::Read };
    let repo = match authorized_repo(&core, &headers, &owner, &repo, action).await {
        Ok(repo) => repo,
        Err(response) => return Ok(*response),
    };
    // Git has no v2 for pushing; a v2 client falls back to v0 when it gets a v0 answer.
    let version = match (service, protocol_version(&headers)) {
        (Service::ReceivePack, Version::V2) => Version::V0,
        (_, version) => version,
    };

    let store = core.store().clone();
    let body = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, ApiError> {
        let repo = store.open(repo.id)?;
        let mut body = Vec::new();
        if version == Version::V2 {
            // Protocol v2 answers start directly with the capability list.
            protocol::write_v2_advertisement(&mut body)?;
            return Ok(body);
        }
        // v0/v1 over HTTP start with this service announcement.
        protocol::pktline::write_line(&mut body, &format!("# service=git-{}", service.name()))?;
        protocol::pktline::write_flush(&mut body)?;
        let version_1 = version == Version::V1;
        let written = match service {
            Service::UploadPack => protocol::write_upload_pack_advertisement(&repo, version_1, &mut body),
            Service::ReceivePack => protocol::write_receive_pack_advertisement(&repo, version_1, &mut body),
        };
        written.map_err(serve_error)?;
        Ok(body)
    })
    .await??;

    let content_type = format!("application/x-git-{}-advertisement", service.name());
    Ok(git_response(&content_type, Body::from(body)))
}

/// Clone and fetch: one negotiation round, and the pack once negotiation is done.
async fn upload_pack(
    State(core): State<Core>,
    State(git_tasks): State<TaskTracker>,
    Path((owner, repo)): Path<(String, String)>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, ApiError> {
    if !has_content_type(&headers, Service::UploadPack) {
        return Ok(StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response());
    }
    let repo = match authorized_repo(&core, &headers, &owner, &repo, Action::Read).await {
        Ok(repo) => repo,
        Err(response) => return Ok(*response),
    };
    let mut request = Vec::new();
    request_reader(&headers, body).take(MAX_FETCH_REQUEST + 1).read_to_end(&mut request).await?;
    if request.len() as u64 > MAX_FETCH_REQUEST {
        return Ok(StatusCode::PAYLOAD_TOO_LARGE.into_response());
    }
    let v2 = protocol_version(&headers) == Version::V2;

    // Packs can be gigabytes; they're never held in memory (NFR-PERF-004).
    let (tx, rx) = mpsc::channel::<io::Result<Bytes>>(4);
    let store = core.store().clone();
    git_tasks.spawn_blocking(move || {
        let interrupt = Arc::new(AtomicBool::new(false));
        let writer = ChannelWriter { tx, interrupt: interrupt.clone() };
        let mut out = BufWriter::with_capacity(64 * 1024, writer);
        let result = store.open(repo.id).map_err(protocol::ServeError::from).and_then(|repo| {
            if v2 {
                protocol::serve_v2(&repo, &request, &mut out, &interrupt)
            } else {
                protocol::serve_upload_pack(&repo, &request, &mut out, &interrupt)
            }
        });
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
    Ok(git_response("application/x-git-upload-pack-result", Body::from_stream(stream)))
}

/// Push: the ref updates and the pack stream in, the report goes out once the
/// pack is checked and the refs are updated.
async fn receive_pack(
    State(core): State<Core>,
    State(git_tasks): State<TaskTracker>,
    Path((owner, repo)): Path<(String, String)>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, ApiError> {
    if !has_content_type(&headers, Service::ReceivePack) {
        return Ok(StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response());
    }
    let repo = match authorized_repo(&core, &headers, &owner, &repo, Action::Write).await {
        Ok(repo) => repo,
        Err(response) => return Ok(*response),
    };
    let mut input = BufReader::with_capacity(64 * 1024, SyncIoBridge::new(request_reader(&headers, body)));
    let store = core.store().clone();
    let report = git_tasks
        .spawn_blocking(move || -> Result<Vec<u8>, ApiError> {
            let git_repo = store.open(repo.id)?;
            let mut out = Vec::new();
            let interrupt = AtomicBool::new(false);
            protocol::serve_receive_pack(
                &git_repo,
                &store.tmp_dir(),
                &mut input,
                &mut out,
                &interrupt,
                &PushHooks,
            )
            .map_err(serve_error)?;
            Ok(out)
        })
        .await??;
    Ok(git_response("application/x-git-receive-pack-result", Body::from(report)))
}

/// The repository, if the request's credentials allow `action` on it. Otherwise
/// the response to send:
/// - anonymous, and either the repository is invisible or the action needs
///   more: `401` with a challenge, so git asks for a token;
/// - signed in but can't see it: `404`, the same as a missing repository
///   (FR-ACL-013);
/// - can see it but not do `action`: `403`.
async fn authorized_repo(
    core: &Core,
    headers: &HeaderMap,
    owner: &str,
    name: &str,
    action: Action,
) -> Result<Repo, Box<Response>> {
    let actor = git_actor(core, headers).await?;
    match core.repo_for(&actor, owner, name, action).await {
        Ok(repo) => Ok(repo),
        Err(Error::RepoNotFound(_) | Error::Forbidden) if actor == Actor::Anonymous => {
            Err(Box::new(challenge()))
        }
        Err(Error::Forbidden) => Err(Box::new(
            (StatusCode::FORBIDDEN, "You don't have permission to push here.\n").into_response(),
        )),
        Err(err) => Err(Box::new(ApiError::from(err).into_response())),
    }
}

/// Klotho's checks around a push. Who may push at all is decided before the
/// push is read (`authorized_repo`); `pre_receive` is where per-ref rules such
/// as branch protection go. `post_receive` is where webhooks and other events
/// will start.
struct PushHooks;

impl ReceiveHooks for PushHooks {
    fn pre_receive(&self, _updates: &[RefUpdate], _push_options: &[String]) -> Result<(), String> {
        Ok(())
    }

    fn post_receive(&self, updates: &[RefUpdate], _push_options: &[String]) {
        for update in updates {
            tracing::debug!(name = %update.name, old = %update.old, new = %update.new, "ref updated by push");
        }
    }
}

fn has_content_type(headers: &HeaderMap, service: Service) -> bool {
    let expected = format!("application/x-git-{}-request", service.name());
    headers.get(header::CONTENT_TYPE).is_some_and(|value| value == expected.as_str())
}

fn serve_error(err: protocol::ServeError) -> ApiError {
    match err {
        protocol::ServeError::Io(err) => err.into(),
        protocol::ServeError::Git(err) => klotho_core::Error::from(err).into(),
        // The engine answers protocol errors in-band; this is only a fallback.
        protocol::ServeError::Protocol(err) => io::Error::other(err.to_string()).into(),
    }
}

/// The request body as a reader, gunzipped if the client compressed it (git
/// gzips large fetch requests, FR-GIT-006).
fn request_reader(headers: &HeaderMap, body: Body) -> Pin<Box<dyn AsyncRead + Send>> {
    let reader = StreamReader::new(body.into_data_stream().map_err(io::Error::other));
    let gzipped = headers.get(header::CONTENT_ENCODING).is_some_and(|v| v == "gzip");
    if gzipped { Box::pin(GzipDecoder::new(reader)) } else { Box::pin(reader) }
}

fn git_response(content_type: &str, body: Body) -> Response {
    (
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "no-cache, max-age=0, must-revalidate"),
        ],
        body,
    )
        .into_response()
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
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn detects_the_protocol_version() {
        let version = |value: Option<&str>| {
            let mut headers = HeaderMap::new();
            if let Some(value) = value {
                headers.insert("git-protocol", HeaderValue::from_str(value).unwrap());
            }
            protocol_version(&headers)
        };
        assert!(version(Some("version=2")) == Version::V2);
        assert!(version(Some("object-format=sha1:version=2")) == Version::V2);
        assert!(version(Some("version=1")) == Version::V1);
        assert!(version(None) == Version::V0);
    }
}
