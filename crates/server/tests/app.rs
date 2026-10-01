//! Routing and middleware checks run against the app in memory, and a graceful
//! shutdown check run over a real TCP connection.

use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use klotho_core::config::ServerConfig;
use klotho_git::{RepoName, RepoStore};
use klotho_server::AppState;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tower::ServiceExt;

fn state() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let store = RepoStore::new(dir.path()).unwrap();
    store.create(&"demo".parse::<RepoName>().unwrap()).unwrap();
    (dir, AppState::new(store))
}

async fn get(uri: &str, request_id: Option<&str>) -> axum::response::Response {
    let (_dir, state) = state();
    let mut request = Request::get(uri);
    if let Some(id) = request_id {
        request = request.header("x-request-id", id);
    }
    klotho_server::build_app(state, &ServerConfig::default())
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn health_is_ok_and_every_response_gets_a_request_id() {
    let response = get("/-/health", None).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().contains_key("x-request-id"));
}

#[tokio::test]
async fn an_incoming_request_id_is_kept() {
    let response = get("/-/health", Some("from-the-proxy")).await;
    assert_eq!(response.headers()["x-request-id"], "from-the-proxy");
}

#[tokio::test]
async fn unknown_api_paths_get_json_not_the_web_ui() {
    let response = get("/api/nope", None).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.headers()["content-type"], "application/json");
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], br#"{"error":"not found"}"#);
}

#[tokio::test]
async fn api_rejects_bodies_over_the_limit() {
    let (_dir, state) = state();
    let config = ServerConfig { api_body_limit: 16, ..ServerConfig::default() };
    let request = Request::post("/api/repos")
        .header("content-type", "application/json")
        .body(Body::from(format!(r#"{{"name":"{}"}}"#, "x".repeat(64))))
        .unwrap();
    let response = klotho_server::build_app(state, &config).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

/// Starts the server on a free port. Returns its address, the trigger that starts
/// shutdown, and the task that ends when the server has stopped.
async fn start(
    grace: Duration,
) -> (
    std::net::SocketAddr,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<std::io::Result<()>>,
    tempfile::TempDir,
) {
    let (dir, state) = state();
    let git_tasks = state.git_tasks.clone();
    let app = klotho_server::build_app(state, &ServerConfig::default());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, stopped) = oneshot::channel();
    let shutdown = async {
        let _ = stopped.await;
    };
    let server = tokio::spawn(klotho_server::serve(listener, app, git_tasks, shutdown, grace));
    (addr, stop, server, dir)
}

/// Sends the headers of a fetch request whose body is still to come, so the
/// request is in flight with a `git upload-pack` waiting on it.
async fn begin_fetch(addr: std::net::SocketAddr) -> TcpStream {
    let mut conn = TcpStream::connect(addr).await.unwrap();
    conn.write_all(
        b"POST /demo/git-upload-pack HTTP/1.1\r\n\
          Host: localhost\r\n\
          Content-Type: application/x-git-upload-pack-request\r\n\
          Transfer-Encoding: chunked\r\n\
          Connection: close\r\n\r\n",
    )
    .await
    .unwrap();
    // Give the server time to start git.
    tokio::time::sleep(Duration::from_millis(300)).await;
    conn
}

#[tokio::test]
async fn shutdown_lets_a_running_git_request_finish() {
    let (addr, stop, server, _dir) = start(Duration::from_secs(30)).await;
    let mut conn = begin_fetch(addr).await;

    stop.send(()).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!server.is_finished(), "the server stopped while a request was running");

    // Finish the request body: a flush packet, so upload-pack has nothing to send.
    conn.write_all(b"4\r\n0000\r\n0\r\n\r\n").await.unwrap();
    let mut response = String::new();
    conn.read_to_string(&mut response).await.unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");

    tokio::time::timeout(Duration::from_secs(10), server).await.unwrap().unwrap().unwrap();
}

#[tokio::test]
async fn shutdown_gives_up_after_the_grace_period() {
    let (addr, stop, server, _dir) = start(Duration::from_millis(300)).await;
    let _conn = begin_fetch(addr).await; // never finished

    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), server).await.unwrap().unwrap().unwrap();
}
