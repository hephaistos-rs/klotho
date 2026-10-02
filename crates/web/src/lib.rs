//! Klotho's web UI: Topcoat pages rendered on the server (ADR 0002).
//!
//! This is the only crate that knows about Topcoat. `klotho-server` mounts
//! [`service`] as axum's fallback, so pages see every request that isn't git
//! transport, `/api` or an operations endpoint. Pages reach the domain through
//! `klotho-core`'s [`Core`], registered as app context.

mod account;
mod session;
mod tokens;
mod ui;

use std::io;
use std::net::SocketAddr;

use klotho_core::Core;
use topcoat::Result;
use topcoat::asset::{AssetBundle, AssetConfig, Manifest, RouterBuilderAssetExt};
use topcoat::context::{Cx, try_app_context};
use topcoat::cookie::RouterBuilderCookieExt;
use topcoat::router::tower::TowerService;
use topcoat::router::{Router, Slot, layout, page};
use topcoat::session::{RouterBuilderSessionExt, SessionConfig};
use topcoat::tailwind;
use topcoat::view::{View, view};

use crate::session::{SessionCookie, current_user};

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
    // Without a bundle (`Assets::None`) there's no stylesheet to link.
    let stylesheet = try_app_context::<AssetConfig>(cx)
        .filter(|config| config.get(tailwind::stylesheet!()).is_some())
        .map(|config| config.resolve(tailwind::stylesheet!()));
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"Klotho"</title>
                if let Some(href) = stylesheet {
                    <link rel="stylesheet" href=(href)>
                }
                topcoat::dev::script()
            </head>
            <body class="bg-white text-gray-900">
                <header class="border-b">
                    <nav class="mx-auto flex max-w-3xl items-center justify-between px-4 py-3">
                        <a href="/" class="font-bold">"Klotho"</a>
                        match user {
                            Some(user) => {
                                <div class="flex items-center gap-4 text-sm">
                                    <span>(user.username)</span>
                                    <a href="/-/settings/tokens">"Access tokens"</a>
                                    <form method="post" action="/-/logout">
                                        <button type="submit">"Sign out"</button>
                                    </form>
                                </div>
                            },
                            None => {
                                <div class="flex items-center gap-4 text-sm">
                                    <a href="/-/login">"Sign in"</a>
                                    <a href="/-/register">"Register"</a>
                                </div>
                            },
                        }
                    </nav>
                </header>
                <main class="mx-auto max-w-3xl px-4 py-8">
                    (slot)
                </main>
            </body>
        </html>
    })
}

#[page("/")]
async fn home() -> Result<impl View> {
    Ok(view! {
        <h1 class="text-3xl font-bold">"Klotho"</h1>
        <p class="mt-2 text-gray-600">"A self-hosted git forge. Browsing repositories arrives in Phase 3."</p>
    })
}
