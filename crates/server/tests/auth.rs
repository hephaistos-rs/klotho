//! Accounts and access control (Phase 2): web sign-in and CSRF protection, API
//! tokens, and git over HTTP with Basic auth, checked against the whole app.

mod common;

use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use axum::response::Response;
use common::{ALICE_PASSWORD, blocking, git, git_try, local_repo_with_commit};
use klotho_core::config::{AuthConfig, Registration, ServerConfig};
use klotho_core::{NewUser, Scope};
use tower::ServiceExt;

async fn app_with(auth: AuthConfig, public_url: &str) -> (tempfile::TempDir, klotho_core::Core, Router) {
    let (dir, state) = common::state_with(auth, public_url).await;
    let core = state.core.clone();
    (dir, core, klotho_server::build_app(state, &ServerConfig::default()))
}

async fn app() -> (tempfile::TempDir, klotho_core::Core, Router) {
    app_with(AuthConfig::default(), "http://localhost:3000").await
}

async fn body(response: Response) -> String {
    String::from_utf8(to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap()
}

/// A browser form post from this site (`Sec-Fetch-Site: same-origin`).
fn form(uri: &str, cookie: Option<&str>, fields: &str) -> Request<Body> {
    form_from(uri, cookie, fields, "same-origin")
}

fn form_from(uri: &str, cookie: Option<&str>, fields: &str, site: &str) -> Request<Body> {
    let mut request = Request::post(uri)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("sec-fetch-site", site);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    request.body(Body::from(fields.to_owned())).unwrap()
}

fn get(uri: &str, cookie: Option<&str>) -> Request<Body> {
    let mut request = Request::get(uri);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    request.body(Body::empty()).unwrap()
}

/// Signs alice in and returns the session cookie (`name=value`).
async fn sign_in(app: &Router) -> String {
    let fields =
        format!("login=alice&password={}&return_to=%2F-%2Fsettings%2Ftokens", encode(ALICE_PASSWORD));
    let response = app.clone().oneshot(form("/-/login", None, &fields)).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()[header::LOCATION], "/-/settings/tokens");
    let set_cookie = response.headers()[header::SET_COOKIE].to_str().unwrap().to_owned();
    assert!(set_cookie.contains("HttpOnly") && set_cookie.contains("SameSite=Lax"), "{set_cookie}");
    set_cookie.split(';').next().unwrap().to_owned()
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

#[tokio::test]
async fn sign_in_create_a_token_and_sign_out() {
    let (_dir, _core, app) = app().await;

    let wrong = form("/-/login", None, "login=alice&password=nope");
    let response = app.clone().oneshot(wrong).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(body(response).await.contains("Incorrect username or password."));

    let cookie = sign_in(&app).await;
    let response = app.clone().oneshot(get("/-/settings/tokens", Some(&cookie))).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(body(response).await.contains("You have no tokens yet."));

    let fields = "name=laptop&repo_read=on&repo_write=on&expires_days=30";
    let response = app.clone().oneshot(form("/-/settings/tokens", Some(&cookie), fields)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let page = body(response).await;
    assert!(page.contains("klotho_pat_"), "the new token is shown once");
    assert!(page.contains("repo:read repo:write"), "{page}");

    let response = app.clone().oneshot(get("/-/settings/tokens", Some(&cookie))).await.unwrap();
    assert!(!body(response).await.contains("klotho_pat_"), "and never again");

    let response = app.clone().oneshot(form("/-/logout", Some(&cookie), "")).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    // The old cookie no longer works: the session is gone from the database.
    let response = app.clone().oneshot(get("/-/settings/tokens", Some(&cookie))).await.unwrap();
    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(response.headers()[header::LOCATION], "/-/login?return_to=/-/settings/tokens");
}

#[tokio::test]
async fn a_cross_site_form_post_with_the_session_cookie_is_refused() {
    let (_dir, core, app) = app().await;
    let cookie = sign_in(&app).await;
    let fields = "name=evil&repo_write=on";

    for site in ["cross-site", "same-site"] {
        let response =
            app.clone().oneshot(form_from("/-/settings/tokens", Some(&cookie), fields, site)).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{site}");
    }
    let alice = core.find_user("alice").await.unwrap();
    assert!(core.list_tokens(alice.id).await.unwrap().is_empty(), "no token was created");
}

#[tokio::test]
async fn the_api_ignores_the_session_cookie() {
    let (_dir, core, app) = app().await;
    let cookie = sign_in(&app).await;

    let response = app.clone().oneshot(get("/api/v1/user", Some(&cookie))).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(response.headers()[header::WWW_AUTHENTICATE].to_str().unwrap().starts_with("Bearer"));

    let token = common::token(&core, "alice", &[Scope::UserRead]).await;
    let request = Request::get("/api/v1/user").header(header::AUTHORIZATION, format!("Bearer {token}"));
    let response = app.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(body(response).await.contains(r#""username":"alice""#));

    let bad = Request::get("/api/v1/user").header(header::AUTHORIZATION, "Bearer klotho_pat_nope");
    let response = app.oneshot(bad.body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "a wrong token is never treated as anonymous");
}

#[tokio::test]
async fn sessions_use_a_host_prefixed_secure_cookie_over_https() {
    let (_dir, _core, app) = app_with(AuthConfig::default(), "https://git.example.com").await;
    let fields = format!("login=alice&password={}", encode(ALICE_PASSWORD));
    let response = app.oneshot(form("/-/login", None, &fields)).await.unwrap();
    let set_cookie = response.headers()[header::SET_COOKIE].to_str().unwrap();
    assert!(set_cookie.starts_with("__Host-klotho_session="), "{set_cookie}");
    assert!(set_cookie.contains("Secure"), "{set_cookie}");
}

#[tokio::test]
async fn registration_follows_the_configured_mode() {
    let fields = "username=bob&email=bob%40example.com&password=bob%27s+password";

    let (_dir, _core, closed) = app().await;
    let response = closed.oneshot(form("/-/register", None, fields)).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(body(response).await.contains("Registration is closed."));

    let open = AuthConfig { registration: Registration::Open, ..AuthConfig::default() };
    let (_dir, core, app) = app_with(open, "http://localhost:3000").await;
    let response = app.oneshot(form("/-/register", None, fields)).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER, "signed up and signed in");
    assert!(response.headers().contains_key(header::SET_COOKIE));
    core.sign_in("bob", "bob's password").await.unwrap();
}

#[tokio::test]
async fn a_private_repository_looks_exactly_like_a_missing_one() {
    let (_dir, core, app) = app().await;
    core.create_repo("alice", "secret", true).await.unwrap();
    core.create_user(NewUser::named("bob")).await.unwrap();
    let bob = common::token(&core, "bob", &[Scope::RepoRead]).await;

    let fetch = |path: &str, token: Option<&str>| {
        let mut request = Request::get(path);
        if let Some(token) = token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        app.clone().oneshot(request.body(Body::empty()).unwrap())
    };
    for token in [None, Some(bob.as_str())] {
        let hidden = fetch("/api/v1/repos/alice/secret", token).await.unwrap();
        let missing = fetch("/api/v1/repos/alice/nothing", token).await.unwrap();
        assert_eq!((hidden.status(), missing.status()), (StatusCode::NOT_FOUND, StatusCode::NOT_FOUND));
        // Identical apart from the name the caller asked for.
        assert_eq!(body(hidden).await.replace("secret", "X"), body(missing).await.replace("nothing", "X"));
    }

    let listed = fetch("/api/v1/users/alice/repos", Some(&bob)).await.unwrap();
    assert!(!body(listed).await.contains("secret"), "not listed either (FR-ACL-011)");
}

#[tokio::test(flavor = "multi_thread")]
async fn git_asks_anonymous_pushers_for_credentials_and_rejects_wrong_ones() {
    let running = common::start(Duration::from_secs(5)).await;
    let anonymous = running.anonymous_url("/alice/demo.git");
    let wrong = format!("http://alice:klotho_pat_wrong@{}/alice/demo.git", running.addr);
    let work = tempfile::tempdir().unwrap();
    let work = work.path().to_owned();

    blocking(move || {
        let local = local_repo_with_commit(&work, "local", "README.md");
        let (ok, stderr) = git_try(&local, &["push", &anonymous, "HEAD:refs/heads/main"]);
        assert!(!ok);
        // git got the 401 challenge and wanted to prompt for a username.
        assert!(stderr.contains("could not read Username"), "{stderr}");

        let (ok, stderr) = git_try(&local, &["push", &wrong, "HEAD:refs/heads/main"]);
        assert!(!ok);
        assert!(stderr.contains("Authentication failed"), "{stderr}");

        // Anyone may clone a public repository, though.
        git(&work, &["clone", "-q", &anonymous, "copy"]);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_only_token_clones_but_cannot_push() {
    let running = common::start(Duration::from_secs(5)).await;
    running.core.create_repo("alice", "private", true).await.unwrap();
    let read_only = common::token(&running.core, "alice", &[Scope::RepoRead]).await;
    let (full, limited) = (
        running.url("/alice/private.git"),
        format!("http://alice:{read_only}@{}/alice/private.git", running.addr),
    );
    let anonymous = running.anonymous_url("/alice/private.git");
    let work = tempfile::tempdir().unwrap();
    let work = work.path().to_owned();

    blocking(move || {
        let local = local_repo_with_commit(&work, "local", "README.md");
        git(&local, &["push", "-q", &full, "HEAD:refs/heads/main"]);

        git(&work, &["clone", "-q", &limited, "copy"]);
        assert!(work.join("copy/README.md").is_file());

        let (ok, stderr) = git_try(&local, &["push", &limited, "HEAD:refs/heads/other"]);
        assert!(!ok);
        assert!(stderr.contains("403"), "{stderr}");

        // Anonymous clones of a private repository are challenged, not shown.
        let (ok, stderr) = git_try(&work, &["clone", &anonymous, "anon"]);
        assert!(!ok && stderr.contains("could not read Username"), "{stderr}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn strangers_get_not_found_from_git_for_private_repositories() {
    let running = common::start(Duration::from_secs(5)).await;
    running.core.create_repo("alice", "private", true).await.unwrap();
    running.core.create_user(NewUser::named("bob")).await.unwrap();
    let bob = common::token(&running.core, "bob", &[Scope::RepoRead, Scope::RepoWrite]).await;
    let url = |name: &str| format!("http://bob:{bob}@{}/alice/{name}.git", running.addr);
    let (hidden, missing) = (url("private"), url("missing"));
    let work = tempfile::tempdir().unwrap();
    let work = work.path().to_owned();

    blocking(move || {
        let (_, hidden_err) = git_try(&work, &["clone", &hidden, "copy"]);
        let (_, missing_err) = git_try(&work, &["clone", &missing, "copy"]);
        assert!(hidden_err.contains("not found"), "{hidden_err}");
        assert_eq!(hidden_err.replace("private", "X"), missing_err.replace("missing", "X"));
    })
    .await;
}
