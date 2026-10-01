//! Routing, middleware and API checks run against the app in memory, and a
//! graceful shutdown check run over a real TCP connection.

mod common;

use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use axum::response::Response;
use klotho_core::config::ServerConfig;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tower::ServiceExt;

async fn send(request: Request<Body>) -> Response {
    let (_dir, state) = common::state().await;
    klotho_server::build_app(state, &ServerConfig::default()).oneshot(request).await.unwrap()
}

async fn get(uri: &str) -> Response {
    send(Request::get(uri).body(Body::empty()).unwrap()).await
}

fn post(uri: &str, body: Value) -> Request<Body> {
    Request::post(uri).header("content-type", "application/json").body(Body::from(body.to_string())).unwrap()
}

async fn json(response: Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}

#[tokio::test]
async fn health_and_ready() {
    let response = get("/-/health").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().contains_key("x-request-id"));
    assert_eq!(get("/-/ready").await.status(), StatusCode::OK);
}

#[tokio::test]
async fn an_incoming_request_id_is_kept() {
    let request = Request::get("/-/health").header("x-request-id", "from-the-proxy").body(Body::empty());
    let response = send(request.unwrap()).await;
    assert_eq!(response.headers()["x-request-id"], "from-the-proxy");
}

#[tokio::test]
async fn unknown_api_paths_get_json_not_the_web_ui() {
    for path in ["/api/nope", "/api/v1/nope", "/api/v2/repos/alice/demo"] {
        let response = get(path).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        assert_eq!(response.headers()["content-type"], "application/json", "{path}");
        assert_eq!(json(response).await, json!({ "code": "not_found", "message": "not found" }), "{path}");
    }
}

#[tokio::test]
async fn api_rejects_bodies_over_the_limit() {
    let (_dir, state) = common::state().await;
    let config = ServerConfig { api_body_limit: 16, ..ServerConfig::default() };
    let request = post("/api/v1/admin/users", json!({ "username": "x".repeat(64) }));
    let response = klotho_server::build_app(state, &config).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn repository_resource_has_the_fr_api_010_fields() {
    let response = get("/api/v1/repos/ALICE/Demo.git").await;
    assert_eq!(response.status(), StatusCode::OK);
    let repo = json(response).await;
    assert_eq!(repo["name"], "demo");
    assert_eq!(repo["full_name"], "alice/demo");
    assert_eq!(repo["owner"]["username"], "alice");
    assert_eq!(repo["private"], false);
    assert_eq!(repo["default_branch"], "main");
    assert_eq!(repo["html_url"], "http://localhost:3000/alice/demo");
    assert_eq!(repo["clone_url"], "http://localhost:3000/alice/demo.git");
    assert_eq!(repo["ssh_url"], "git@localhost:alice/demo.git");
    assert!(repo["id"].is_i64());
    assert!(repo["created_at"].as_str().unwrap().ends_with('Z'));
}

#[tokio::test]
async fn creating_keeps_the_display_name_and_rejects_reserved_suffixes() {
    let (_dir, state) = common::state().await;
    let app = klotho_server::build_app(state, &ServerConfig::default());

    let created = app.clone().oneshot(post("/api/v1/admin/users/alice/repos", json!({ "name": "MyRepo" })));
    let response = created.await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(json(response).await["name"], "MyRepo");

    let duplicate = app.clone().oneshot(post("/api/v1/admin/users/alice/repos", json!({ "name": "myrepo" })));
    let response = duplicate.await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(json(response).await["code"], "repo_exists");

    let reserved = app.oneshot(post("/api/v1/admin/users/alice/repos", json!({ "name": "demo.git" })));
    let response = reserved.await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json(response).await["code"], "name_invalid");
}

#[tokio::test]
async fn missing_repositories_and_owners_are_404_with_a_code() {
    for path in ["/api/v1/repos/alice/nope", "/api/v1/repos/nobody/demo"] {
        let response = get(path).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        assert_eq!(json(response).await["code"], "repo_not_found", "{path}");
    }
}

#[tokio::test]
async fn listing_is_paged_with_a_link_header() {
    let (_dir, state) = common::state().await;
    for name in ["b", "c"] {
        state.core.create_repo("alice", name).await.unwrap();
    }
    let app = klotho_server::build_app(state, &ServerConfig::default());

    let first =
        app.clone().oneshot(Request::get("/api/v1/users/alice/repos?limit=2").body(Body::empty()).unwrap());
    let response = first.await.unwrap();
    let link = response.headers()["link"].to_str().unwrap().to_owned();
    assert_eq!(link, r#"<http://localhost:3000/api/v1/users/alice/repos?limit=2&cursor=c>; rel="next""#);
    let names: Vec<Value> =
        json(response).await.as_array().unwrap().iter().map(|r| r["name"].clone()).collect();
    assert_eq!(names, [json!("b"), json!("c")]);

    let next =
        app.oneshot(Request::get("/api/v1/users/alice/repos?limit=2&cursor=c").body(Body::empty()).unwrap());
    let response = next.await.unwrap();
    assert!(!response.headers().contains_key("link"));
    assert_eq!(json(response).await.as_array().unwrap().len(), 1);
}

/// Sends the headers of a fetch request whose body is still to come, so the
/// request is in flight with a `git upload-pack` waiting on it.
async fn begin_fetch(addr: std::net::SocketAddr) -> TcpStream {
    let mut conn = TcpStream::connect(addr).await.unwrap();
    conn.write_all(
        b"POST /alice/demo.git/git-upload-pack HTTP/1.1\r\n\
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
    let running = common::start(Duration::from_secs(30)).await;
    let mut conn = begin_fetch(running.addr).await;

    running.stop.send(()).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!running.server.is_finished(), "the server stopped while a request was running");

    // Finish the request body: a flush packet, so upload-pack has nothing to send.
    conn.write_all(b"4\r\n0000\r\n0\r\n\r\n").await.unwrap();
    let mut response = String::new();
    conn.read_to_string(&mut response).await.unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");

    tokio::time::timeout(Duration::from_secs(10), running.server).await.unwrap().unwrap().unwrap();
}

#[tokio::test]
async fn shutdown_gives_up_after_the_grace_period() {
    let running = common::start(Duration::from_millis(300)).await;
    let _conn = begin_fetch(running.addr).await; // never finished

    running.stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), running.server).await.unwrap().unwrap().unwrap();
}
