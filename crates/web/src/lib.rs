//! Klotho's web UI: Topcoat pages rendered on the server (ADR 0002).
//!
//! This is the only crate that knows about Topcoat. `klotho-server` mounts
//! [`service`] as axum's fallback, so pages see every request that isn't git
//! transport, `/api` or an operations endpoint.

use std::io;
use std::net::SocketAddr;

use topcoat::Result;
use topcoat::asset::{AssetBundle, AssetConfig, Manifest, RouterBuilderAssetExt};
use topcoat::router::tower::TowerService;
use topcoat::router::{Router, Slot, layout, page};
use topcoat::tailwind;
use topcoat::view::{View, view};

/// Where the asset bundle (the stylesheet and Topcoat's runtime script) comes from.
pub enum Assets {
    /// The bundle `topcoat dev` or `topcoat asset bundle` writes next to the
    /// executable, served by Topcoat under `/_topcoat/assets`. For development.
    NextToBinary,
    /// Files the caller serves itself at `base_url`. Only the bundle's
    /// `manifest.toml` is needed here, to turn assets into URLs. Release builds
    /// embed both in the binary (`cargo xtask dist`).
    Hosted { base_url: &'static str, manifest: &'static str },
}

/// The web UI as a tower service.
///
/// Fails when the asset bundle can't be loaded. Rendering a page without it would
/// panic on the first asset, so callers should not serve pages in that case.
pub fn service(assets: Assets) -> io::Result<TowerService> {
    let assets = match assets {
        Assets::NextToBinary => AssetConfig::from(AssetBundle::load()?),
        Assets::Hosted { base_url, manifest } => {
            AssetConfig::hosted_at(base_url, Manifest::parse(manifest).map_err(io::Error::other)?)
        }
    };
    let router = Router::builder().layout(root_layout).page(home).assets(assets).build();
    Ok(TowerService::new(router))
}

/// Tells `topcoat dev` the server is up, so it can open and refresh pages.
/// Does nothing outside `topcoat dev`.
pub async fn notify_dev_server(addr: SocketAddr) {
    topcoat::dev::notify_ready(Some(addr)).await;
}

#[layout("/")]
async fn root_layout(slot: Slot<'_>) -> Result<impl View> {
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"Klotho"</title>
                <link rel="stylesheet" href=(tailwind::stylesheet!())>
                topcoat::dev::script()
            </head>
            <body class="bg-white text-gray-900">
                (slot)
            </body>
        </html>
    })
}

#[page("/")]
async fn home() -> Result<impl View> {
    Ok(view! {
        <main class="mx-auto max-w-2xl px-4 py-16">
            <h1 class="text-3xl font-bold">"Klotho"</h1>
            <p class="mt-2 text-gray-600">"A self-hosted git forge. The web UI arrives in Phase 3."</p>
        </main>
    })
}
