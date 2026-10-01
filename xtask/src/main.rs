//! Project tasks: `cargo xtask <task>`.
//!
//! - `dev`: runs Klotho under `topcoat dev` with `klotho.dev.toml`. It rebuilds,
//!   re-bundles assets and restarts the server on every save.
//! - `dist`: the release build, a single `klotho` binary with the web UI's assets
//!   embedded (ADR 0002), written to `dist/`.
//!
//! - `sqlx-prepare`: regenerates `.sqlx/` after a query or migration changes.
//!
//! `dev` and `dist` need the Topcoat CLI: `cargo install topcoat-cli --version 0.9.0 --locked`.
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
        _ => Err("usage: cargo xtask <dev|dist|sqlx-prepare>".into()),
    }
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
    sqlx(&["migrate", "run", "--source", "crates/core/migrations"])?;
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
///    home page and its stylesheet. If the second pass changed any asset's ID, the
///    page fails to render, so this catches a broken bundle before release.
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
        let href = stylesheet_href(&page.body).ok_or("the home page links no stylesheet")?;
        let css = get(port, &href)?;
        expect(&css, "200", "text/css")?;
        if !css.headers.contains("immutable") {
            return Err(format!("{href} isn't served with immutable caching").into());
        }
        println!("dist check: / and {href} served by the binary alone");
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

fn stylesheet_href(html: &str) -> Option<String> {
    let link = &html[html.find("rel=\"stylesheet\"")?..];
    let href = &link[link.find("href=\"")? + "href=\"".len()..];
    Some(href[..href.find('"')?].to_owned())
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
    fn finds_the_stylesheet_link() {
        let html = r#"<head><link rel="stylesheet" href="/-/assets/tailwind-abc.css"></head>"#;
        assert_eq!(stylesheet_href(html).as_deref(), Some("/-/assets/tailwind-abc.css"));
        assert_eq!(stylesheet_href("<p>no styles</p>"), None);
    }
}
