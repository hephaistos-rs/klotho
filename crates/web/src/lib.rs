//! Klotho's web UI: Topcoat pages rendered on the server (ADR 0002).
//!
//! This is the only crate that knows about Topcoat. `klotho-server` mounts
//! [`service`] as axum's fallback, so pages see every request that isn't git
//! transport, `/api` or an operations endpoint. Pages reach the domain through
//! `klotho-core`'s [`Core`], registered as app context.

mod account;
/// Topcoat UI components, copied in with `topcoat ui add` and ours to edit.
/// Public while pages are still adopting them, so the ones no page uses yet
/// don't warn as dead code. The design run removes unused ones and makes
/// this private again at its end.
pub mod components;
mod session;
mod theme;
mod tokens;
mod ui;

use std::io;
use std::net::SocketAddr;

use klotho_core::Core;
use topcoat::Result;
use topcoat::asset::{AssetBundle, AssetConfig, Manifest, RouterBuilderAssetExt};
use topcoat::context::{Cx, try_app_context};
use topcoat::cookie::RouterBuilderCookieExt;
use topcoat::font::RouterBuilderFontExt;
use topcoat::router::request::uri;
use topcoat::router::tower::TowerService;
use topcoat::router::{Router, Slot, layout, page};
use topcoat::session::{RouterBuilderSessionExt, SessionConfig};
use topcoat::tailwind;
use topcoat::view::{View, view};

use crate::session::{SessionCookie, current_user};
use crate::theme::current_theme;
use crate::ui::shell;

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
        Assets::NextToBinary => AssetConfig::from(AssetBundle::load()?),
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
        .font(theme::SANS)
        .font(theme::MONO)
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
                <meta name="color-scheme" content="light dark">
                <title>"Klotho"</title>
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
                    <link rel="stylesheet" href=(href)>
                }
                topcoat::dev::script()
            </head>
            <body>shell(user: user, theme: theme, path: path, (slot))</body>
        </html>
    })
}

#[page("/")]
async fn home() -> Result<impl View> {
    Ok(view! {
        <h1 class="text-3xl font-semibold tracking-tight">"Klotho"</h1>
        <p class="mt-2 text-muted-foreground">
            "A self-hosted git forge. Browsing repositories arrives in Phase 3."
        </p>
    })
}
