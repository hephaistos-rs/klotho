//! JSON API for managing and browsing repositories.
//!
//! | Method | Path                          | Query                 |
//! |--------|-------------------------------|-----------------------|
//! | GET    | `/api/repos`                  |                       |
//! | POST   | `/api/repos`                  | body: `{"name": ..}`  |
//! | GET    | `/api/repos/{repo}`           |                       |
//! | GET    | `/api/repos/{repo}/commits`   | `rev`, `limit`        |
//! | GET    | `/api/repos/{repo}/tree`      | `rev`, `path`         |
//! | GET    | `/api/repos/{repo}/raw`       | `rev`, `path`         |

use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use klotho_git::{CommitInfo, RepoInfo, RepoName, RepoStore, TreeEntryInfo};
use serde::Deserialize;

use crate::AppState;
use crate::error::AppError;

const MAX_COMMITS: usize = 500;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/repos", get(list_repos).post(create_repo))
        .route("/repos/{repo}", get(repo_info))
        .route("/repos/{repo}/commits", get(commits))
        .route("/repos/{repo}/tree", get(tree))
        .route("/repos/{repo}/raw", get(raw))
        // Unknown API paths get JSON, not the web UI's HTML 404 (FR-UI-050).
        .fallback(|| async { (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "not found" }))) })
}

#[derive(Deserialize)]
struct CreateRepo {
    name: String,
}

#[derive(Deserialize)]
struct RevQuery {
    #[serde(default = "default_rev")]
    rev: String,
    #[serde(default)]
    path: String,
    limit: Option<usize>,
}

fn default_rev() -> String {
    "HEAD".to_owned()
}

/// gix is synchronous, so repository work runs on the blocking thread pool.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> klotho_git::Result<T> + Send + 'static,
) -> Result<T, AppError> {
    Ok(tokio::task::spawn_blocking(f).await??)
}

async fn list_repos(State(store): State<RepoStore>) -> Result<Json<Vec<String>>, AppError> {
    let names = blocking(move || store.list()).await?;
    Ok(Json(names.iter().map(ToString::to_string).collect()))
}

async fn create_repo(
    State(store): State<RepoStore>,
    Json(body): Json<CreateRepo>,
) -> Result<(StatusCode, Json<RepoInfo>), AppError> {
    let name: RepoName = body.name.parse()?;
    let info = blocking(move || RepoInfo::read(&store.create(&name)?, &name)).await?;
    Ok((StatusCode::CREATED, Json(info)))
}

async fn repo_info(
    State(store): State<RepoStore>,
    Path(repo): Path<String>,
) -> Result<Json<RepoInfo>, AppError> {
    let name: RepoName = repo.parse()?;
    Ok(Json(blocking(move || RepoInfo::read(&store.open(&name)?, &name)).await?))
}

async fn commits(
    State(store): State<RepoStore>,
    Path(repo): Path<String>,
    Query(query): Query<RevQuery>,
) -> Result<Json<Vec<CommitInfo>>, AppError> {
    let name: RepoName = repo.parse()?;
    let limit = query.limit.unwrap_or(30).min(MAX_COMMITS);
    let commits = blocking(move || CommitInfo::log(&store.open(&name)?, &query.rev, limit)).await?;
    Ok(Json(commits))
}

async fn tree(
    State(store): State<RepoStore>,
    Path(repo): Path<String>,
    Query(query): Query<RevQuery>,
) -> Result<Json<Vec<TreeEntryInfo>>, AppError> {
    let name: RepoName = repo.parse()?;
    let entries = blocking(move || TreeEntryInfo::list(&store.open(&name)?, &query.rev, &query.path)).await?;
    Ok(Json(entries))
}

async fn raw(
    State(store): State<RepoStore>,
    Path(repo): Path<String>,
    Query(query): Query<RevQuery>,
) -> Result<impl IntoResponse, AppError> {
    let name: RepoName = repo.parse()?;
    let data = blocking(move || klotho_git::read_blob(&store.open(&name)?, &query.rev, &query.path)).await?;
    // Always octet-stream so a stored HTML file can't run as a page on this origin.
    Ok(([(header::CONTENT_TYPE, "application/octet-stream")], data))
}
