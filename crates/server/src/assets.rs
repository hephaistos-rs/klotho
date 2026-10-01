//! The web UI and its asset bundle (ADR 0002).
//!
//! In development the bundle sits next to the executable (`topcoat dev` or
//! `topcoat asset bundle` puts it there) and Topcoat serves it. Release builds
//! (`cargo xtask dist`) embed the bundle and serve it from `/-/assets/` with
//! immutable caching (FR-UI-052).

use std::convert::Infallible;
use std::io;

use axum::Router;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use tower::Service;

use crate::AppState;

#[cfg(not(embed_assets))]
pub fn router() -> Router<AppState> {
    mount(klotho_web::service(klotho_web::Assets::NextToBinary), Router::new())
}

#[cfg(embed_assets)]
pub fn router() -> Router<AppState> {
    let assets = klotho_web::Assets::Hosted { base_url: embedded::BASE_URL, manifest: embedded::MANIFEST };
    let routes = Router::new().route("/-/assets/{*file}", axum::routing::get(embedded::serve));
    mount(klotho_web::service(assets), routes)
}

/// Adds the web UI as the fallback. Without its asset bundle, pages would panic
/// on their first asset, so the UI is replaced by a 503 while git and the API
/// keep working.
fn mount<S>(web: io::Result<S>, routes: Router<AppState>) -> Router<AppState>
where
    S: Service<Request, Error = Infallible> + Clone + Send + Sync + 'static,
    S::Response: IntoResponse,
    S::Future: Send + 'static,
{
    match web {
        Ok(web) => routes.fallback_service(web),
        Err(err) => {
            tracing::warn!(
                %err,
                "web UI disabled: no asset bundle next to the binary. Use `topcoat dev`, or run `topcoat asset bundle` after building"
            );
            routes.fallback(|| async {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "The web UI's asset bundle wasn't found. See the server log.",
                )
            })
        }
    }
}

#[cfg(embed_assets)]
mod embedded {
    use axum::extract::Path;
    use axum::http::{StatusCode, header};
    use axum::response::{IntoResponse, Response};

    pub const BASE_URL: &str = "/-/assets";
    pub const MANIFEST: &str = include_str!(concat!(env!("KLOTHO_ASSETS_DIR"), "/manifest.toml"));

    #[derive(rust_embed::Embed)]
    #[folder = "$KLOTHO_ASSETS_DIR"]
    struct Bundle;

    pub async fn serve(Path(file): Path<String>) -> Response {
        let Some(content) = Bundle::get(&file) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let mime = mime_guess::from_path(&file).first_or_octet_stream();
        (
            [
                (header::CONTENT_TYPE, mime.as_ref()),
                // Filenames carry a content hash, so they never change.
                (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            content.data,
        )
            .into_response()
    }
}
