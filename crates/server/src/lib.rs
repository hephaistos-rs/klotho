//! Klotho's HTTP server: the JSON API and git smart HTTP.

mod api;
mod error;
mod git_http;

use axum::Router;
use klotho_git::RepoStore;

pub fn build_app(store: RepoStore) -> Router {
    Router::new()
        .nest("/api", api::router())
        .merge(git_http::router())
        .with_state(store)
}
