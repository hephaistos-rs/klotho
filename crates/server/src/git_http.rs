//! Git's "smart HTTP" protocol, so `git clone/fetch/push http://host/<repo>.git` work.
//!
//! gix has no server-side protocol support, so like `git http-backend` this hands
//! each request to `git upload-pack` (clone/fetch) or `git receive-pack` (push)
//! and pipes the request and response bodies through.
//!
//! See <https://git-scm.com/docs/http-protocol>.
//!
//! TODO: there is no authentication yet, so anyone who can reach the server can push.

use std::path::Path as FsPath;
use std::pin::Pin;
use std::process::Stdio;

use async_compression::tokio::bufread::GzipDecoder;
use axum::Router;
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use futures_util::TryStreamExt;
use klotho_git::{RepoName, RepoStore};
use serde::Deserialize;
use tokio::io::AsyncRead;
use tokio::process::Command;
use tokio_util::io::{ReaderStream, StreamReader};

use crate::error::AppError;

pub fn router() -> Router<RepoStore> {
    Router::new()
        .route("/{repo}/info/refs", get(info_refs))
        .route("/{repo}/git-upload-pack", post(|s, p, h, b| rpc(Service::UploadPack, s, p, h, b)))
        .route("/{repo}/git-receive-pack", post(|s, p, h, b| rpc(Service::ReceivePack, s, p, h, b)))
        // Pushes can be arbitrarily large.
        .layer(DefaultBodyLimit::disable())
}

#[derive(Clone, Copy)]
enum Service {
    UploadPack,
    ReceivePack,
}

impl Service {
    fn from_query(service: &str) -> Option<Self> {
        match service {
            "git-upload-pack" => Some(Self::UploadPack),
            "git-receive-pack" => Some(Self::ReceivePack),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::UploadPack => "upload-pack",
            Self::ReceivePack => "receive-pack",
        }
    }

    fn command(self, repo: &FsPath, protocol: Option<&str>, advertise_refs: bool) -> Command {
        let mut cmd = Command::new("git");
        cmd.arg(self.name()).arg("--stateless-rpc");
        if advertise_refs {
            cmd.arg("--advertise-refs");
        }
        cmd.arg(repo);
        if let Some(protocol) = protocol {
            cmd.env("GIT_PROTOCOL", protocol);
        }
        cmd
    }
}

#[derive(Deserialize)]
struct InfoRefsQuery {
    service: Option<String>,
}

/// First request of every clone/fetch/push: lists the repo's refs and capabilities.
async fn info_refs(
    State(store): State<RepoStore>,
    Path(repo): Path<String>,
    Query(query): Query<InfoRefsQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let Some(service) = query.service.as_deref().and_then(Service::from_query) else {
        return Ok((StatusCode::FORBIDDEN, "only the smart HTTP protocol is supported").into_response());
    };
    let repo_path = repo_path(&store, &repo)?;
    let protocol = git_protocol(&headers);

    let output = service
        .command(&repo_path, protocol, true)
        .stderr(Stdio::inherit())
        .output()
        .await?;
    if !output.status.success() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "git {} --advertise-refs exited with {}",
            service.name(),
            output.status
        )));
    }

    // Protocol v2 responses start directly with the capability list; v0/v1 need
    // this service announcement first (matching git http-backend).
    let mut body = Vec::new();
    if !protocol.is_some_and(|p| p.contains("version=2")) {
        body.extend(pkt_line(&format!("# service=git-{}\n", service.name())));
        body.extend(b"0000");
    }
    body.extend(output.stdout);

    Ok(git_response(&format!("application/x-git-{}-advertisement", service.name()), Body::from(body)))
}

/// The actual fetch negotiation or push, streamed through the git process.
async fn rpc(
    service: Service,
    State(store): State<RepoStore>,
    Path(repo): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, AppError> {
    let repo_path = repo_path(&store, &repo)?;
    let expected = format!("application/x-git-{}-request", service.name());
    if headers.get(header::CONTENT_TYPE).is_none_or(|v| v != expected.as_str()) {
        return Ok(StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response());
    }

    let mut child = service
        .command(&repo_path, git_protocol(&headers), false)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let stdout = child.stdout.take().expect("stdout is piped");

    // git clients gzip large fetch requests.
    let reader = StreamReader::new(body.into_data_stream().map_err(std::io::Error::other));
    let gzipped = headers.get(header::CONTENT_ENCODING).is_some_and(|v| v == "gzip");
    let mut input: Pin<Box<dyn AsyncRead + Send>> = if gzipped {
        Box::pin(GzipDecoder::new(reader))
    } else {
        Box::pin(reader)
    };

    // Feed the request in the background while the response streams out, and
    // close stdin when done so git knows the request is complete.
    tokio::spawn(async move {
        if let Err(err) = tokio::io::copy(&mut input, &mut stdin).await {
            tracing::warn!(%err, "failed to forward request body to git");
        }
    });
    tokio::spawn(async move {
        match child.wait().await {
            Ok(status) if !status.success() => tracing::warn!(%status, "git {} failed", service.name()),
            Err(err) => tracing::warn!(%err, "failed to wait for git {}", service.name()),
            Ok(_) => {}
        }
    });

    let content_type = format!("application/x-git-{}-result", service.name());
    Ok(git_response(&content_type, Body::from_stream(ReaderStream::new(stdout))))
}

fn repo_path(store: &RepoStore, repo: &str) -> Result<std::path::PathBuf, AppError> {
    let name: RepoName = repo.parse()?;
    if !store.exists(&name) {
        return Err(klotho_git::Error::RepoNotFound(name.to_string()).into());
    }
    Ok(store.path(&name))
}

/// The client's `Git-Protocol` header, passed to git as `GIT_PROTOCOL` so protocol
/// v2 works. Only forwarded if it looks like `version=2`-style key/values.
fn git_protocol(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get("git-protocol")?.to_str().ok()?;
    let safe = value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'=' | b':' | b'.' | b'-' | b'_'));
    safe.then_some(value)
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

/// Encodes one pkt-line: a 4-hex-digit length (including itself) then the data.
fn pkt_line(data: &str) -> Vec<u8> {
    format!("{:04x}{data}", data.len() + 4).into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkt_line_includes_its_own_length() {
        assert_eq!(pkt_line("# service=git-upload-pack\n"), b"001e# service=git-upload-pack\n");
    }
}
