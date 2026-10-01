//! The JSON API, `/api/v1` (docs/design/api-endpoints.md).
//!
//! | Method      | Path                                       | Notes                       |
//! |-------------|--------------------------------------------|-----------------------------|
//! | GET         | `/repos/{owner}/{repo}`                    | FR-API-010 resource         |
//! | GET         | `/repos/{owner}/{repo}/commits`            | `rev`, `limit`              |
//! | GET         | `/repos/{owner}/{repo}/tree`               | `rev`, `path`               |
//! | GET         | `/repos/{owner}/{repo}/raw`                | `rev`, `path`               |
//! | GET         | `/users/{username}/repos`                  | `limit`, `cursor`; `Link`   |
//! | POST        | `/admin/users`                             | `{"username"}`              |
//! | POST        | `/admin/users/{username}/repos`            | `{"name"}`                  |
//! | GET         | `/admin/unadopted`                         | FR-STOR-020                 |
//! | POST/DELETE | `/admin/unadopted/{*path}`                 | adopt `{"owner", "name"?}`  |
//!
//! There's no authentication until Phase 2, so the admin endpoints are open to
//! anyone who can reach the server, like pushing is.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use klotho_core::{Core, Owner, Repo, StorageReport};
use klotho_git::{CommitInfo, RepoInfo, TreeEntryInfo};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::error::ApiError;

const MAX_COMMITS: usize = 500;
/// Page sizes (FR-API-020).
const DEFAULT_PAGE: u32 = 30;
const MAX_PAGE: u32 = 100;

/// The `/api` router: version 1, plus JSON 404s for anything else under `/api`
/// so it never falls through to the web UI's HTML pages (FR-UI-050).
pub fn router() -> Router<AppState> {
    let v1 = Router::new()
        .route("/repos/{owner}/{repo}", get(repo))
        .route("/repos/{owner}/{repo}/commits", get(commits))
        .route("/repos/{owner}/{repo}/tree", get(tree))
        .route("/repos/{owner}/{repo}/raw", get(raw))
        .route("/users/{username}/repos", get(user_repos))
        .route("/admin/users", post(create_user))
        .route("/admin/users/{username}/repos", post(create_repo))
        .route("/admin/unadopted", get(unadopted))
        .route("/admin/unadopted/{*path}", post(adopt).delete(delete_unadopted));
    Router::new().nest("/v1", v1).fallback(|| async { ApiError::not_found() })
}

/// A repository as the API returns it (FR-API-010).
#[derive(Serialize)]
struct RepoResource {
    id: i64,
    owner: Owner,
    name: String,
    full_name: String,
    /// Always false until visibility arrives in Phase 2.
    private: bool,
    /// The branch HEAD points at, even if it has no commits yet.
    default_branch: Option<String>,
    empty: bool,
    html_url: String,
    clone_url: String,
    ssh_url: String,
    created_at: jiff::Timestamp,
}

impl RepoResource {
    async fn build(core: &Core, repo: Repo) -> Result<Self, ApiError> {
        let info = core.with_git(&repo, RepoInfo::read).await?;
        let full_name = repo.full_name();
        let urls = core.urls();
        Ok(Self {
            id: repo.id,
            html_url: urls.html(&full_name),
            clone_url: urls.clone(&full_name),
            ssh_url: urls.ssh(&full_name),
            owner: repo.owner,
            name: repo.name,
            full_name,
            private: false,
            default_branch: info.default_branch,
            empty: info.empty,
            created_at: repo.created_at,
        })
    }
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

async fn repo(
    State(core): State<Core>,
    Path((owner, name)): Path<(String, String)>,
) -> Result<Json<RepoResource>, ApiError> {
    let repo = core.find_repo(&owner, &name).await?;
    Ok(Json(RepoResource::build(&core, repo).await?))
}

async fn commits(
    State(core): State<Core>,
    Path((owner, name)): Path<(String, String)>,
    Query(query): Query<RevQuery>,
) -> Result<Json<Vec<CommitInfo>>, ApiError> {
    let repo = core.find_repo(&owner, &name).await?;
    let limit = query.limit.unwrap_or(30).min(MAX_COMMITS);
    Ok(Json(core.with_git(&repo, move |git| CommitInfo::log(git, &query.rev, limit)).await?))
}

async fn tree(
    State(core): State<Core>,
    Path((owner, name)): Path<(String, String)>,
    Query(query): Query<RevQuery>,
) -> Result<Json<Vec<TreeEntryInfo>>, ApiError> {
    let repo = core.find_repo(&owner, &name).await?;
    Ok(Json(core.with_git(&repo, move |git| TreeEntryInfo::list(git, &query.rev, &query.path)).await?))
}

async fn raw(
    State(core): State<Core>,
    Path((owner, name)): Path<(String, String)>,
    Query(query): Query<RevQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let repo = core.find_repo(&owner, &name).await?;
    let data = core.with_git(&repo, move |git| klotho_git::read_blob(git, &query.rev, &query.path)).await?;
    // Always octet-stream with nosniff, so a pushed HTML file can't run as a page
    // on this origin (NFR-SEC-010).
    Ok((
        [(header::CONTENT_TYPE, "application/octet-stream"), (header::X_CONTENT_TYPE_OPTIONS, "nosniff")],
        data,
    ))
}

#[derive(Deserialize)]
struct PageQuery {
    limit: Option<u32>,
    /// From the previous page's `Link: <…>; rel="next"`.
    cursor: Option<String>,
}

/// An owner's repositories, one page at a time, with an RFC 8288 `Link` header
/// to the next page (FR-API-020, 021).
async fn user_repos(
    State(core): State<Core>,
    Path(username): Path<String>,
    Query(page): Query<PageQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = page.limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
    let repos = core.list_repos(&username, page.cursor.as_deref(), limit).await?;

    let mut headers = HeaderMap::new();
    if repos.len() == limit as usize
        && let Some(last) = repos.last()
    {
        let cursor = last.name.to_ascii_lowercase();
        let next = core.urls().api(&format!("/users/{username}/repos?limit={limit}&cursor={cursor}"));
        if let Ok(link) = HeaderValue::from_str(&format!("<{next}>; rel=\"next\"")) {
            headers.insert(header::LINK, link);
        }
    }
    let mut resources = Vec::with_capacity(repos.len());
    for repo in repos {
        resources.push(RepoResource::build(&core, repo).await?);
    }
    Ok((headers, Json(resources)))
}

#[derive(Deserialize)]
struct CreateUser {
    username: String,
}

async fn create_user(
    State(core): State<Core>,
    Json(body): Json<CreateUser>,
) -> Result<(StatusCode, Json<Owner>), ApiError> {
    Ok((StatusCode::CREATED, Json(core.create_user(&body.username).await?)))
}

#[derive(Deserialize)]
struct CreateRepo {
    name: String,
}

/// Creates a repository owned by `username`, as Gitea's
/// `POST /admin/users/{username}/repos` does.
async fn create_repo(
    State(core): State<Core>,
    Path(username): Path<String>,
    Json(body): Json<CreateRepo>,
) -> Result<(StatusCode, Json<RepoResource>), ApiError> {
    let repo = core.create_repo(&username, &body.name).await?;
    Ok((StatusCode::CREATED, Json(RepoResource::build(&core, repo).await?)))
}

#[derive(Serialize)]
struct StorageReportResource {
    unadopted: Vec<String>,
    missing: Vec<String>,
}

async fn unadopted(State(core): State<Core>) -> Result<Json<StorageReportResource>, ApiError> {
    let StorageReport { unadopted, missing } = core.storage_report().await?;
    Ok(Json(StorageReportResource { unadopted, missing: missing.iter().map(Repo::full_name).collect() }))
}

#[derive(Deserialize)]
struct Adopt {
    owner: String,
    name: Option<String>,
}

async fn adopt(
    State(core): State<Core>,
    Path(path): Path<String>,
    Json(body): Json<Adopt>,
) -> Result<(StatusCode, Json<RepoResource>), ApiError> {
    let repo = core.adopt(&path, &body.owner, body.name.as_deref()).await?;
    Ok((StatusCode::CREATED, Json(RepoResource::build(&core, repo).await?)))
}

async fn delete_unadopted(
    State(core): State<Core>,
    Path(path): Path<String>,
) -> Result<StatusCode, ApiError> {
    core.delete_unadopted(&path).await?;
    Ok(StatusCode::NO_CONTENT)
}
