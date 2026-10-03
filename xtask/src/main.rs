//! Project tasks: `cargo xtask <task>`.
//!
//! - `dev`: runs Klotho under `topcoat dev` with `klotho.dev.toml`. It rebuilds,
//!   re-bundles assets and restarts the server on every save.
//! - `dist`: the release build, a single `klotho` binary with the web UI's assets
//!   embedded (ADR 0002), written to `dist/`.
//!
//! - `sqlx-prepare`: regenerates `.sqlx/` after a query or migration changes.
//! - `ci`: every check a change must pass, stopping at the first failure: format,
//!   clippy, tests, `cargo deny` and `dist`. Run it by hand before pushing; Lachesis
//!   will run the same command.
//!
//! `dev`, `dist` and `ci` need the Topcoat CLI: `cargo install topcoat-cli --version 0.9.0 --locked`.
//! `ci` also needs cargo-deny (`cargo install cargo-deny --locked`) and the `git` client, which
//! the end-to-end tests drive.
//! `sqlx-prepare` needs sqlx-cli: `cargo install sqlx-cli --version 0.9.0 --no-default-features --features sqlite --locked`.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use std::{env, fs};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result {
    match env::args().nth(1).as_deref() {
        Some("dev") => dev(),
        Some("dist") => dist(),
        Some("sqlx-prepare") => sqlx_prepare(),
        Some("ci") => ci(),
        _ => Err("usage: cargo xtask <ci|dev|dist|sqlx-prepare>".into()),
    }
}

/// The checks every change must pass, in order from fastest to slowest, so a
/// formatting slip fails in seconds rather than after the test suite.
fn ci() -> Result {
    let root = workspace_root();
    let cargo = || {
        let mut cmd = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
        cmd.current_dir(&root);
        cmd
    };
    let start = Instant::now();
    run(cargo().args(["fmt", "--all", "--check"]))?;
    run(cargo().args(["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]))?;
    run(cargo().args(["test", "--workspace"]))?;
    run(cargo().args(["deny", "check"]))?;
    dist()?;
    println!("\nci: all checks passed in {}s", start.elapsed().as_secs());
    Ok(())
}

/// Regenerates `.sqlx/`, the cached query metadata that lets `sqlx::query!`
/// type-check queries without a database (in CI, and for anyone who just builds).
/// Run it after changing a query or a migration, and commit the result.
fn sqlx_prepare() -> Result {
    let root = workspace_root();
    let target = env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let db = target.join("sqlx-prepare.db");
    for file in [db.clone(), db.with_extension("db-wal"), db.with_extension("db-shm")] {
        let _ = fs::remove_file(file);
    }
    let url = format!("sqlite:{}", db.display().to_string().replace('\\', "/"));
    let sqlx = |args: &[&str]| -> Result {
        run(Command::new("sqlx").args(args).env("DATABASE_URL", &url).current_dir(&root))
    };
    sqlx(&["database", "create"])?;
    sqlx(&["migrate", "run", "--source", "crates/core/migrations/sqlite"])?;
    run(Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["sqlx", "prepare", "--workspace", "--", "--all-targets"])
        .env("DATABASE_URL", &url)
        .current_dir(&root))
}

fn dev() -> Result {
    let root = workspace_root();
    run(Command::new("topcoat")
        .args(["dev", "--package", "klotho"])
        .env("KLOTHO_CONFIG", root.join("klotho.dev.toml"))
        .current_dir(&root))
}

/// The two-pass release build from ADR 0002:
///
/// 1. `topcoat asset bundle --release` builds Klotho and collects the assets its
///    binary declares into `target/klotho-assets`.
/// 2. `cargo build --release` with `KLOTHO_ASSETS_DIR` pointing at the bundle
///    builds it again with the bundle embedded. Only `klotho-server` (whose
///    build script reads the variable) and the binary are rebuilt; a Cargo
///    feature would change dependency resolution and with it the asset IDs.
/// 3. The binary is copied alone into an empty folder, started, and must serve the
///    home page, every stylesheet and preloaded file it links, and every file those
///    stylesheets load (fonts), all from itself with immutable caching. The page
///    may not reference another host. If the second pass changed any asset's ID,
///    the page fails to render, so this catches a broken bundle before release.
fn dist() -> Result {
    let root = workspace_root();
    let target = env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let bundle = target.join("klotho-assets");
    if bundle.exists() {
        fs::remove_dir_all(&bundle)?;
    }

    run(Command::new("topcoat")
        .args(["asset", "bundle", "--release", "--package", "klotho", "--out"])
        .arg(&bundle)
        .env_remove("KLOTHO_ASSETS_DIR")
        .current_dir(&root))?;
    run(Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["build", "--release", "--package", "klotho"])
        .env("KLOTHO_ASSETS_DIR", &bundle)
        .current_dir(&root))?;

    let exe = format!("klotho{}", env::consts::EXE_SUFFIX);
    let dist = root.join("dist");
    fs::create_dir_all(&dist)?;
    fs::copy(target.join("release").join(&exe), dist.join(&exe))?;

    smoke_test(&dist.join(&exe))?;
    println!("\ndist: {} is ready", dist.join(&exe).display());
    Ok(())
}

fn smoke_test(binary: &Path) -> Result {
    let dir = env::temp_dir().join(format!("klotho-dist-check-{}", std::process::id()));
    fs::create_dir_all(&dir)?;
    let exe = dir.join(binary.file_name().unwrap());
    fs::copy(binary, &exe)?;

    let port = TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
    let server = KillOnDrop(
        Command::new(&exe)
            .arg("serve")
            .current_dir(&dir)
            .env_remove("KLOTHO_CONFIG")
            .env("KLOTHO_SERVER__ADDR", format!("127.0.0.1:{port}"))
            .env("KLOTHO_STORAGE__DATA_DIR", dir.join("data"))
            .stdout(Stdio::null())
            .spawn()?,
    );

    let result = (|| -> Result {
        let page = get_when_up(port, "/")?;
        expect(&page, "200", "text/html")?;
        if let Some(url) =
            urls(&page.body).into_iter().find(|url| !url.starts_with('/') || url.starts_with("//"))
        {
            return Err(format!("the home page references another host: {url}").into());
        }
        let stylesheets = links(&page.body, "stylesheet");
        if !stylesheets.iter().any(|href| href.starts_with("/-/assets/")) {
            return Err("the home page links no stylesheet from the embedded bundle".into());
        }
        let mut files = links(&page.body, "preload");
        for href in &stylesheets {
            let css = get(port, href)?;
            expect(&css, "200", "text/css")?;
            immutable(href, &css)?;
            files.extend(css_urls(&css.body));
        }
        for href in &files {
            let file = get(port, href)?;
            expect(&file, "200", "")?;
            immutable(href, &file)?;
        }
        println!(
            "dist check: /, {} stylesheets and {} files they load served by the binary alone",
            stylesheets.len(),
            files.len()
        );
        Ok(())
    })();

    drop(server);
    let _ = fs::remove_dir_all(&dir);
    result
}

struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Response {
    status_line: String,
    headers: String,
    body: String,
}

fn immutable(href: &str, response: &Response) -> Result {
    if response.headers.contains("immutable") {
        Ok(())
    } else {
        Err(format!("{href} isn't served with immutable caching").into())
    }
}

fn expect(response: &Response, status: &str, content_type: &str) -> Result {
    let ok = response.status_line.split(' ').nth(1) == Some(status)
        && response.headers.to_ascii_lowercase().contains(&format!("content-type: {content_type}"));
    if ok {
        Ok(())
    } else {
        Err(format!("expected {status} {content_type}, got:\n{}\n{}", response.status_line, response.headers)
            .into())
    }
}

/// Retries until the server has started, for up to 30 seconds.
fn get_when_up(port: u16, path: &str) -> Result<Response> {
    let start = Instant::now();
    loop {
        match get(port, path) {
            Ok(response) => return Ok(response),
            Err(_) if start.elapsed() < Duration::from_secs(30) => {
                std::thread::sleep(Duration::from_millis(200))
            }
            Err(err) => return Err(format!("the server didn't answer: {err}").into()),
        }
    }
}

/// A minimal HTTP/1.1 GET, so xtask needs no dependencies.
fn get(port: u16, path: &str) -> Result<Response> {
    let mut conn = TcpStream::connect(("127.0.0.1", port))?;
    conn.set_read_timeout(Some(Duration::from_secs(10)))?;
    write!(conn, "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
    let mut raw = Vec::new();
    conn.read_to_end(&mut raw)?;
    let raw = String::from_utf8_lossy(&raw);
    let (head, body) = raw.split_once("\r\n\r\n").ok_or("malformed HTTP response")?;
    let (status_line, headers) = head.split_once("\r\n").unwrap_or((head, ""));
    Ok(Response { status_line: status_line.to_owned(), headers: headers.to_owned(), body: body.to_owned() })
}

/// The `href` of every `<link rel="{rel}">` in `html`.
fn links(html: &str, rel: &str) -> Vec<String> {
    html.split("<link")
        .skip(1)
        .map(|tag| &tag[..tag.find('>').unwrap_or(tag.len())])
        .filter(|tag| tag.contains(&format!("rel=\"{rel}\"")))
        .filter_map(|tag| attr(tag, "href"))
        .collect()
}

/// Every `href` and `src` value in `html`.
fn urls(html: &str) -> Vec<String> {
    let mut found = Vec::new();
    for name in ["href", "src"] {
        let needle = format!(" {name}=\"");
        let mut rest = html;
        while let Some(start) = rest.find(&needle) {
            rest = &rest[start + needle.len()..];
            let end = rest.find('"').unwrap_or(rest.len());
            found.push(rest[..end].to_owned());
        }
    }
    found.retain(|url| !url.starts_with('#'));
    found
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let needle = format!(" {name}=\"");
    let value = &tag[tag.find(&needle)? + needle.len()..];
    Some(value[..value.find('"')?].to_owned())
}

/// Every `url(...)` in a stylesheet, unquoted.
fn css_urls(css: &str) -> Vec<String> {
    css.split("url(")
        .skip(1)
        .filter_map(|rest| rest.find(')').map(|end| rest[..end].trim_matches(['"', '\'']).to_owned()))
        .filter(|url| !url.starts_with("data:"))
        .collect()
}

fn run(cmd: &mut Command) -> Result {
    println!("> {cmd:?}");
    let status = cmd.status().map_err(|err| format!("couldn't run {:?}: {err}", cmd.get_program()))?;
    if status.success() { Ok(()) } else { Err(format!("{:?} failed: {status}", cmd.get_program()).into()) }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_links_urls_and_css_urls() {
        let html = concat!(
            r#"<head><link rel="preload" href="/-/assets/a.woff2" as="font">"#,
            r#"<link rel="stylesheet" href="/-/assets/tailwind-abc.css"></head>"#,
            r##"<a href="/x">x</a><a href="#main">skip</a>"##,
        );
        assert_eq!(links(html, "stylesheet"), ["/-/assets/tailwind-abc.css"]);
        assert_eq!(links(html, "preload"), ["/-/assets/a.woff2"]);
        assert_eq!(urls(html), ["/-/assets/a.woff2", "/-/assets/tailwind-abc.css", "/x"]);
        assert!(links("<p>no styles</p>", "stylesheet").is_empty());
        let css = r#"@font-face{src:url("/-/assets/n.woff2") format("woff2")}a{background:url(data:x)}"#;
        assert_eq!(css_urls(css), ["/-/assets/n.woff2"]);
    }
}
