//! Klotho's web UI: Topcoat pages rendered on the server (ADR 0002).
//!
//! This is the only crate that knows about Topcoat. `klotho-server` mounts
//! [`service`] as axum's fallback, so pages see every request that isn't git
//! transport, `/api` or an operations endpoint. Pages reach the domain through
//! `klotho-core`'s [`Core`], registered as app context.

mod account;
mod code;
/// Topcoat UI components, copied in with `topcoat ui add` and ours to edit.
/// Only the ones a page uses are kept; `topcoat ui add` brings one back.
mod components;
mod history;
mod owner;
mod repo;
mod session;
mod theme;
mod tokens;
mod ui;

use std::io;
use std::net::SocketAddr;

use klotho_core::{Actor, Core};
use topcoat::Result;
use topcoat::asset::{AssetBundle, AssetConfig, Manifest, RouterBuilderAssetExt};
use topcoat::context::{Cx, try_app_context};
use topcoat::cookie::RouterBuilderCookieExt;
use topcoat::font::RouterBuilderFontExt;
use topcoat::router::error::NotFoundError;
use topcoat::router::header::{
    CONTENT_SECURITY_POLICY, REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use topcoat::router::request::uri;
use topcoat::router::response::Response;
use topcoat::router::tower::TowerService;
use topcoat::router::{Body, HeaderValue, Next, Router, Slot, StatusCode, layer, layout, page};
use topcoat::session::{RouterBuilderSessionExt, SessionConfig};
use topcoat::tailwind;
use topcoat::view::{View, component, error_boundary, view};

use crate::owner::repo_list;
use crate::session::{SessionCookie, core, current_user};
use crate::theme::current_theme;
use crate::ui::{empty_state, page_header, section_heading, shell};

/// The web UI as a tower service, as [`service`] returns it.
pub type WebService = TowerService;

/// Where the asset bundle (the stylesheet and Topcoat's runtime script) comes from.
pub enum Assets {
    /// The bundle `topcoat dev` or `topcoat asset bundle` writes next to the
    /// executable, served by Topcoat under `/_topcoat/assets`. For development.
    NextToBinary,
    /// Files the caller serves itself at `base_url`. Only the bundle's
    /// `manifest.toml` is needed here, to turn assets into URLs. Release builds
    /// embed both in the binary (`cargo xtask dist`).
    Hosted { base_url: &'static str, manifest: &'static str },
    /// No bundle: pages render without styles. For tests, and for a binary
    /// started without its bundle.
    None,
}

/// The web UI as a tower service. Fails when the asset bundle can't be loaded.
pub fn service(assets: Assets, core: Core) -> io::Result<TowerService> {
    let assets = match assets {
        Assets::NextToBinary => {
            let config = AssetConfig::from(AssetBundle::load()?);
            // A bundle from an older build has other asset IDs, and pages
            // would quietly render without styles.
            if config.get(tailwind::stylesheet!()).is_none() {
                tracing::warn!(
                    "the asset bundle next to the binary is out of date, so pages have no styles. Use `cargo xtask dev`, or run `topcoat asset bundle` after building"
                );
            }
            config
        }
        Assets::Hosted { base_url, manifest } => {
            AssetConfig::hosted_at(base_url, Manifest::parse(manifest).map_err(io::Error::other)?)
        }
        Assets::None => AssetConfig::hosted_at(
            "/-/assets",
            Manifest::parse("version = 1\nassets = []").expect("an empty manifest parses"),
        ),
    };
    // `__Host-` and `Secure` only work over HTTPS; plain HTTP is for localhost.
    let secure = core.urls().is_https();
    let sessions = SessionConfig::builder()
        .token_store(SessionCookie { secure })
        // The cookie lives as long as a session can; idle expiry is checked
        // against the `sessions` table on every request (FR-AUTH-041).
        .lifetime(core.session_max())
        .build();
    let router = Router::builder()
        .cookies()
        .sessions(sessions)
        .app_context(core)
        .layer(security_headers)
        .layout(root_layout)
        .page(home)
        .page(account::login_page)
        .page(account::login_submit)
        .page(account::register_page)
        .page(account::register_submit)
        .route(account::logout)
        .page(tokens::tokens_page)
        .page(tokens::create_token)
        .route(tokens::revoke_token)
        .route(theme::set_theme)
        .page(owner::owner_page)
        .page(code::repo_home)
        .page(code::tree)
        .page(code::blob)
        .route(code::raw)
        .route(history::commits_default)
        .page(history::commits)
        .page(history::branches)
        .page(history::tags)
        .page(not_found_page)
        .page(not_found_in_repo)
        .font(theme::SANS)
        .font(theme::MONO)
        .font(theme::DISPLAY)
        .assets(assets)
        .build();
    Ok(TowerService::new(router))
}

/// Tells `topcoat dev` the server is up, so it can open and refresh pages.
/// Does nothing outside `topcoat dev`.
pub async fn notify_dev_server(addr: SocketAddr) {
    topcoat::dev::notify_ready(Some(addr)).await;
}

#[layout("/")]
async fn root_layout(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
    let user = current_user(cx).await.cloned();
    let theme = current_theme(cx);
    let path = uri(cx).path().to_owned();
    let title = document_title(&path);
    // Without a bundle (`Assets::None`) there's no stylesheet or font file to
    // link, and resolving an asset the bundle lacks would panic.
    let bundled =
        try_app_context::<AssetConfig>(cx).filter(|config| config.get(tailwind::stylesheet!()).is_some());
    let stylesheet = bundled.map(|config| config.resolve(tailwind::stylesheet!()));
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en" class=(theme.html_class())>
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <meta name="color-scheme" content=(theme.color_scheme())>
                <title>(title)</title>
                if let Some(href) = stylesheet {
                    <link
                        rel="preload"
                        href=(theme::SANS_LATIN)
                        as="font"
                        type="font/woff2"
                        crossorigin="anonymous"
                    >
                    topcoat::font::link(font: theme::SANS, preload: false)
                    topcoat::font::link(font: theme::MONO, preload: false)
                    topcoat::font::link(font: theme::DISPLAY, preload: false)
                    <link rel="stylesheet" href=(href)>
                }
                topcoat::dev::script()
            </head>
            <body>
                shell(
                    user: user,
                    theme: theme,
                    path: path,
                    error_boundary(
                        fallback: |error| {
                            if error.downcast_ref::<NotFoundError>().is_none() {
                                return Err(error);
                            }
                            Ok(view! { cx => not_found() })
                        },
                        (slot)
                    )
                )
            </body>
        </html>
    })
}

/// The `<title>` for a path: the page's name, then "Klotho" (WCAG 2.4.2).
/// Repository and owner pages are named after the path they show, which
/// says no more than the address itself.
fn document_title(path: &str) -> String {
    let named = match path {
        "/" => None,
        "/-/login" => Some("Sign in".to_owned()),
        "/-/register" => Some("Create an account".to_owned()),
        "/-/settings/tokens" => Some("Access tokens".to_owned()),
        path if path.starts_with("/-/") || path.starts_with("/_") => None,
        path => {
            let mut segments = path.split('/').filter(|s| !s.is_empty());
            match (segments.next(), segments.next()) {
                (Some(owner), Some(repo)) => Some(format!("{}/{}", decode(owner), decode(repo))),
                (Some(owner), None) => Some(decode(owner)),
                _ => None,
            }
        }
    };
    named.map_or_else(|| "Klotho".to_owned(), |name| format!("{name} · Klotho"))
}

/// Undoes percent-encoding in a path segment, for display.
fn decode(segment: &str) -> String {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && let (Some(hi), Some(lo)) =
                (bytes.get(i + 1).and_then(|&b| hex(b)), bytes.get(i + 2).and_then(|&b| hex(b)))
        {
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Repositories on the home page; the rest are on the owner page.
const HOME_REPOS: u32 = 50;

#[page("/")]
async fn home(cx: &Cx) -> Result<impl View> {
    let user = current_user(cx).await.cloned();
    let mut all = None;
    let repos = match &user {
        Some(user) => {
            let actor = Actor::Session(user.clone());
            // One more than is shown, to know whether to link to the rest.
            let mut repos = core(cx).list_repos(&actor, &user.username, None, HOME_REPOS + 1).await?;
            if repos.len() > HOME_REPOS as usize {
                repos.truncate(HOME_REPOS as usize);
                all = Some(format!("/{}", klotho_core::encode_path(&user.username)));
            }
            Some(repos)
        }
        None => None,
    };
    Ok(view! {
        page_header(title: "Klotho", description: "A self-hosted git forge.")
        match repos {
            Some(repos) => {
                section_heading(title: "Your repositories", id: "your-repositories")
                repo_list(repos: repos, empty: "You have no repositories yet.")
                if let Some(all) = all {
                    <p class="mt-4"><a href=(all)>"All your repositories"</a></p>
                }
                <p class="mt-4 text-sm text-muted-foreground">
                    "Clone and push with git, using an "
                    <a href="/-/settings/tokens">"access token"</a>
                    " as the password."
                </p>
            }
            None => {
                empty_state(
                    title: "Sign in to see your repositories.",
                    <a href="/-/login">"Sign in"</a>
                    " to create an access token for pushing, or for cloning private repositories."
                )
            }
        }
    })
}

/// Every path no page claims gets the shell and a way home, with a 404
/// status. The layout renders the same page for a page's `not_found()`.
///
/// There is no root `/{*path}`: the router can't hold one beside the owner
/// page `/{owner}`. One- and two-segment paths are owner and repository pages
/// (which 404 themselves), so these two cover everything else.
#[page("/-/{*rest}")]
async fn not_found_page() -> Result<impl View> {
    Ok(view! { not_found() })
}

#[page("/{owner_name}/{repo_name}/{*rest}")]
async fn not_found_in_repo() -> Result<impl View> {
    Ok(view! { not_found() })
}

#[component]
async fn not_found() -> Result<impl View> {
    Ok(view! {
        (StatusCode::NOT_FOUND)
        page_header(
            title: "Page not found",
            description: "Nothing lives at this address, or you can't see it."
        )
        <p><a href="/">"Go to the home page"</a></p>
    })
}

/// Headers for every UI response: pages can't be framed by other sites
/// (clickjacking the one-click forms, NFR-SEC-012), content types aren't
/// sniffed, and full URLs don't leak to other sites. A page can set its own
/// `Referrer-Policy` (the magic-link pages need `no-referrer`) and its own
/// `Content-Security-Policy` (raw files are sandboxed).
#[layer("/")]
async fn security_headers(cx: &Cx, body: Body, next: Next<'_>) -> Result<Response> {
    let mut response = next.run(cx, body).await?;
    let headers = response.headers_mut();
    headers.entry(CONTENT_SECURITY_POLICY).or_insert(HeaderValue::from_static("frame-ancestors 'self'"));
    headers.insert(X_FRAME_OPTIONS, HeaderValue::from_static("SAMEORIGIN"));
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.entry(REFERRER_POLICY).or_insert(HeaderValue::from_static("same-origin"));
    Ok(response)
}
