//! Reading repositories through the API and the web pages (Phase 3): refs,
//! history, contents, raw files and the README, against a repository pushed
//! with the real `git` client.

mod common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use axum::response::Response;
use klotho_core::config::ServerConfig;
use serde_json::Value;
use tower::ServiceExt;

use common::{blocking, git};

/// alice's public `demo` with three commits on `main`, branches `feature/x`
/// and `a+b`, a tag `v1`, a README linking into the repository, and her
/// private `secret`. Returns the app and the id of `main`'s tip.
async fn fixture() -> (tempfile::TempDir, Router, String) {
    let (dir, state) = common::state().await;
    let core = state.core.clone();
    core.create_repo("alice", "secret", true).await.unwrap();
    let demo = core.store().path(core.find_repo("alice", "demo").await.unwrap().id);
    let secret = core.store().path(core.find_repo("alice", "secret").await.unwrap().id);
    let work = dir.path().join("work");

    let tip = blocking(move || {
        std::fs::create_dir_all(work.join("docs")).unwrap();
        std::fs::create_dir_all(work.join("src")).unwrap();
        git(&work, &["init", "-q"]);
        std::fs::write(
            work.join("README.md"),
            "# Demo\n\nRead the [guide](docs/guide.md).\n\n<script>alert(1)</script>\n",
        )
        .unwrap();
        git(&work, &["add", "."]);
        git(&work, &["commit", "-q", "-m", "Add the README"]);
        std::fs::write(work.join("docs/guide.md"), "# Guide\n").unwrap();
        git(&work, &["add", "."]);
        git(&work, &["commit", "-q", "-m", "Add the guide"]);
        git(&work, &["tag", "v1"]);
        std::fs::write(work.join("src/lib.rs"), "pub fn answer() -> u32 {\n    42\n}\n").unwrap();
        std::fs::write(work.join("logo.bin"), [0u8, 159, 146, 150, 0, 1, 2]).unwrap();
        git(&work, &["add", "."]);
        git(&work, &["commit", "-q", "-m", "Add the library\n\nWith a body."]);
        git(&work, &["branch", "feature/x"]);
        git(&work, &["branch", "a+b"]);
        let demo = demo.to_string_lossy().into_owned();
        git(&work, &["push", "-q", &demo, "main", "feature/x", "a+b", "v1"]);
        let secret = secret.to_string_lossy().into_owned();
        git(&work, &["push", "-q", &secret, "main"]);
        git(&work, &["rev-parse", "HEAD"]).trim().to_owned()
    })
    .await;
    (dir, klotho_server::build_app(state, &ServerConfig::default()), tip)
}

async fn get(app: &Router, uri: &str) -> Response {
    app.clone().oneshot(Request::get(uri).body(Body::empty()).unwrap()).await.unwrap()
}

async fn body(response: Response) -> String {
    String::from_utf8(to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap()
}

/// A page's text without its markup. Topcoat doesn't keep attribute order
/// stable between renders, so pages are compared by what they say.
fn text(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

async fn json(response: Response) -> Value {
    serde_json::from_str(&body(response).await).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_api_lists_refs_and_pages_history() {
    let (_dir, app, tip) = fixture().await;

    let branches = json(get(&app, "/api/v1/repos/alice/demo/branches").await).await;
    let names: Vec<&str> = branches.as_array().unwrap().iter().map(|b| b["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["a+b", "feature/x", "main"]);
    let main = json(get(&app, "/api/v1/repos/alice/demo/branches/main").await).await;
    assert_eq!(main["commit"]["id"], tip.as_str());
    // The `Link` cursor survives a `+`, which a query decoder reads as a space.
    let mut paged = Vec::new();
    let mut next = Some("/api/v1/repos/alice/demo/branches?limit=1".to_owned());
    while let Some(uri) = next {
        let page = get(&app, &uri).await;
        next = page.headers().get("link").map(|link| {
            let link = link.to_str().unwrap();
            link.trim_start_matches("<http://localhost:3000").split('>').next().unwrap().to_owned()
        });
        paged.extend(
            json(page).await.as_array().unwrap().iter().map(|b| b["name"].as_str().unwrap().to_owned()),
        );
        assert!(paged.len() <= names.len(), "the cursor repeated a page: {paged:?}");
    }
    assert_eq!(paged, names);

    let feature = get(&app, "/api/v1/repos/alice/demo/branches/feature/x").await;
    assert_eq!(feature.status(), StatusCode::OK);

    let tags = json(get(&app, "/api/v1/repos/alice/demo/tags").await).await;
    assert_eq!(tags[0]["name"], "v1");
    assert_eq!(get(&app, "/api/v1/repos/alice/demo/tags/v2").await.status(), StatusCode::NOT_FOUND);

    let first = get(&app, "/api/v1/repos/alice/demo/commits?limit=2").await;
    let link = first.headers()["link"].to_str().unwrap().to_owned();
    assert!(
        link.starts_with("<http://localhost:3000/api/v1/repos/alice/demo/commits?limit=2&cursor="),
        "{link}"
    );
    let page = json(first).await;
    assert_eq!(page[0]["summary"], "Add the library");
    assert_eq!(page.as_array().unwrap().len(), 2);
    let next = link.trim_start_matches("<http://localhost:3000").split('>').next().unwrap().to_owned();
    let rest = get(&app, &next).await;
    assert!(!rest.headers().contains_key("link"));
    assert_eq!(json(rest).await[0]["summary"], "Add the README");

    let at_tag = json(get(&app, "/api/v1/repos/alice/demo/commits?ref=v1").await).await;
    assert_eq!(at_tag.as_array().unwrap().len(), 2);
    let by_path = json(get(&app, "/api/v1/repos/alice/demo/commits?path=src").await).await;
    assert_eq!(by_path.as_array().unwrap().len(), 1);

    let resolved = json(get(&app, "/api/v1/repos/alice/demo/resolve?spec=feature/x/src/lib.rs").await).await;
    assert_eq!(resolved["ref"]["name"], "feature/x");
    assert_eq!(resolved["path"], "src/lib.rs");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_api_reads_contents_raw_files_and_the_readme() {
    let (_dir, app, _tip) = fixture().await;

    let root = json(get(&app, "/api/v1/repos/alice/demo/contents").await).await;
    let names: Vec<&str> =
        root["entries"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"docs") && names.contains(&"README.md"), "{names:?}");
    let file = json(get(&app, "/api/v1/repos/alice/demo/contents/src/lib.rs?ref=feature/x").await).await;
    assert!(file["text"].as_str().unwrap().contains("42"), "{file}");
    assert_eq!(
        get(&app, "/api/v1/repos/alice/demo/contents/src/lib.rs?ref=v1").await.status(),
        StatusCode::NOT_FOUND
    );

    let raw = get(&app, "/api/v1/repos/alice/demo/raw/src/lib.rs").await;
    assert_eq!(raw.headers()["content-type"], "application/octet-stream");
    assert_eq!(raw.headers()["x-content-type-options"], "nosniff");
    assert!(body(raw).await.contains("pub fn answer"));

    let readme = json(get(&app, "/api/v1/repos/alice/demo/readme").await).await;
    let html = readme["html"].as_str().unwrap();
    assert!(html.contains("<h1>Demo</h1>"), "{html}");
    assert!(html.contains("href=\"http://localhost:3000/alice/demo/-/blob/main/docs/guide.md\""), "{html}");
    assert!(!html.contains("<script"), "{html}");
    let none = get(&app, "/api/v1/repos/alice/demo/readme?dir=src").await;
    assert_eq!(none.status(), StatusCode::NOT_FOUND);
    assert_eq!(json(none).await["code"], "readme_not_found");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_private_repository_reads_as_missing_to_strangers() {
    let (_dir, app, _tip) = fixture().await;
    for path in ["branches", "tags", "commits", "contents", "raw/README.md", "readme", "resolve?spec=main"] {
        let private = get(&app, &format!("/api/v1/repos/alice/secret/{path}")).await;
        let missing = get(&app, &format!("/api/v1/repos/alice/nothing/{path}")).await;
        assert_eq!(private.status(), StatusCode::NOT_FOUND, "{path}");
        assert_eq!(body(private).await.replace("secret", "nothing"), body(missing).await, "{path}");
    }
}

/// Signs alice in through the login form and returns the session cookie.
async fn sign_in(app: &Router) -> String {
    let password: String = common::ALICE_PASSWORD
        .bytes()
        .map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect();
    let request = Request::post("/-/login")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("sec-fetch-site", "same-origin")
        .body(Body::from(format!("login=alice&password={password}")))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    response.headers()["set-cookie"].to_str().unwrap().split(';').next().unwrap().to_owned()
}

async fn get_as(app: &Router, uri: &str, cookie: &str) -> Response {
    let request = Request::get(uri).header("cookie", cookie).body(Body::empty()).unwrap();
    app.clone().oneshot(request).await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_repository_pages_show_files_the_readme_and_history() {
    let (_dir, app, tip) = fixture().await;

    let home = get(&app, "/alice/demo").await;
    assert_eq!(home.status(), StatusCode::OK);
    let page = body(home).await;
    assert!(page.contains("<title>alice/demo · Klotho</title>"), "{page}");
    assert!(page.contains("href=\"/alice/demo/-/tree/main/docs\""), "{page}");
    assert!(page.contains("href=\"/alice/demo/-/blob/main/README.md\""), "{page}");
    assert!(page.contains("<h1>Demo</h1>"), "the README is rendered");
    assert!(page.contains("http://localhost:3000/alice/demo/-/blob/main/docs/guide.md"), "{page}");
    assert!(!page.contains("<script>alert"), "the README's script is gone");
    assert!(page.contains("http://localhost:3000/alice/demo.git"), "the clone URL is shown");

    let folder = body(get(&app, "/alice/demo/-/tree/feature/x/src").await).await;
    assert!(folder.contains("href=\"/alice/demo/-/blob/feature/x/src/lib.rs\""), "{folder}");
    let file_as_tree = get(&app, "/alice/demo/-/tree/main/src/lib.rs").await;
    assert_eq!(file_as_tree.status(), StatusCode::SEE_OTHER);
    assert_eq!(file_as_tree.headers()["location"], "/alice/demo/-/blob/main/src/lib.rs");

    let file = body(get(&app, "/alice/demo/-/blob/main/src/lib.rs").await).await;
    assert!(file.contains("id=\"L2\""), "lines can be linked");
    assert!(file.contains("    42"), "{file}");
    assert!(file.contains("href=\"/alice/demo/-/raw/main/src/lib.rs\""), "{file}");
    let binary = body(get(&app, "/alice/demo/-/blob/main/logo.bin").await).await;
    assert!(binary.contains("This file is binary"), "{binary}");
    let readme = body(get(&app, "/alice/demo/-/blob/v1/README.md").await).await;
    assert!(readme.contains("<h1>Demo</h1>"), "Markdown files are rendered");
    let source = body(get(&app, "/alice/demo/-/blob/v1/README.md?plain=1").await).await;
    assert!(source.contains("# Demo"), "and their source is a click away");

    let raw = get(&app, "/alice/demo/-/raw/main/src/lib.rs").await;
    assert_eq!(raw.status(), StatusCode::OK);
    assert_eq!(raw.headers()["content-type"], "application/octet-stream");
    assert_eq!(raw.headers()["x-content-type-options"], "nosniff");
    assert!(raw.headers()["content-security-policy"].to_str().unwrap().contains("sandbox"));
    assert!(body(raw).await.contains("pub fn answer"));

    let bare = get(&app, "/alice/demo/-/commits").await;
    assert_eq!(bare.status(), StatusCode::SEE_OTHER);
    assert_eq!(bare.headers()["location"], "/alice/demo/-/commits/main");
    let log = body(get(&app, "/alice/demo/-/commits/main").await).await;
    for summary in ["Add the library", "Add the guide", "Add the README"] {
        assert!(log.contains(summary), "{summary}");
    }
    assert!(log.contains(&format!("href=\"/alice/demo/-/tree/{tip}\"")), "{log}");
    let at_tag = body(get(&app, "/alice/demo/-/commits/v1").await).await;
    assert!(!at_tag.contains("Add the library") && at_tag.contains("Add the guide"));
    let bad_cursor = get(&app, "/alice/demo/-/commits/main?cursor=nonsense").await;
    assert_eq!(bad_cursor.status(), StatusCode::BAD_REQUEST);
    let by_path = body(get(&app, "/alice/demo/-/commits/main/src").await).await;
    assert!(by_path.contains("Add the library") && !by_path.contains("Add the guide"));

    let branches = body(get(&app, "/alice/demo/-/branches").await).await;
    assert!(branches.contains("feature/x") && branches.contains("Default"), "{branches}");
    let tags = body(get(&app, "/alice/demo/-/tags").await).await;
    assert!(tags.contains("href=\"/alice/demo/-/tree/v1\""), "{tags}");

    let owner = body(get(&app, "/alice").await).await;
    assert!(owner.contains("href=\"/alice/demo\"") && !owner.contains("secret"), "{owner}");
    assert_eq!(get(&app, "/alice/demo/-/blob/main/nothing.txt").await.status(), StatusCode::NOT_FOUND);
    assert_eq!(get(&app, "/alice/demo/-/tree/nope").await.status(), StatusCode::NOT_FOUND);
    assert_eq!(get(&app, "/nobody").await.status(), StatusCode::NOT_FOUND);
    let missing = get(&app, "/someone/something").await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert!(body(missing).await.contains("Page not found"), "the branded HTML 404");
    assert_eq!(get(&app, "/alice/demo/-/nope").await.status(), StatusCode::NOT_FOUND);
    assert_eq!(get(&app, "/alice/demo/a/b/c").await.status(), StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn private_repository_pages_look_missing_until_the_owner_signs_in() {
    let (_dir, app, _tip) = fixture().await;
    for path in [
        "",
        "/-/tree/main",
        "/-/blob/main/README.md",
        "/-/raw/main/README.md",
        "/-/commits/main",
        "/-/branches",
        "/-/tags",
    ] {
        let private = get(&app, &format!("/alice/secret{path}")).await;
        let missing = get(&app, &format!("/alice/nothing{path}")).await;
        assert_eq!(private.status(), StatusCode::NOT_FOUND, "{path}");
        assert_eq!(
            text(&body(private).await.replace("secret", "nothing")),
            text(&body(missing).await),
            "{path}"
        );
    }

    let cookie = sign_in(&app).await;
    let page = get_as(&app, "/alice/secret", &cookie).await;
    assert_eq!(page.status(), StatusCode::OK);
    assert!(body(page).await.contains("Private"));
    let home = body(get_as(&app, "/", &cookie).await).await;
    assert!(home.contains("href=\"/alice/secret\"") && home.contains("href=\"/alice/demo\""), "{home}");
    let owner = body(get_as(&app, "/alice", &cookie).await).await;
    assert!(owner.contains("href=\"/alice/secret\""), "{owner}");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_empty_repository_explains_how_to_push() {
    let (_dir, state) = common::state().await;
    let app = klotho_server::build_app(state, &ServerConfig::default());
    let page = body(get(&app, "/alice/demo").await).await;
    assert!(page.contains("This repository is empty."), "{page}");
    assert!(page.contains("origin main"), "{page}");
    assert!(body(get(&app, "/alice/demo/-/commits/main").await).await.contains("No commits yet."));
}
