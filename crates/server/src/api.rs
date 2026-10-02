//! The JSON API, `/api/v1` (docs/design/api-endpoints.md). Every request is
//! authenticated with `Authorization: Bearer <token>` or is anonymous, and every
//! handler goes through `klotho-core`'s permission checks (FR-ACL-050).
//!
//! | Method        | Path                                 | Needs                         |
//! |---------------|--------------------------------------|-------------------------------|
//! | GET           | `/instance`                          | nothing                       |
//! | GET, PATCH    | `/repos/{owner}/{repo}`              | read; admin + `repo:admin`    |
//! | GET           | `/repos/{owner}/{repo}/commits`      | read (`rev`, `limit`)         |
//! | GET           | `/repos/{owner}/{repo}/tree`         | read (`rev`, `path`)          |
//! | GET           | `/repos/{owner}/{repo}/raw`          | read (`rev`, `path`)          |
//! | GET           | `/users/{username}/repos`            | what you can see; `Link`      |
//! | GET           | `/user`                              | `user:read`                   |
//! | POST          | `/user/repos`                        | `repo:admin`                  |
//! | GET           | `/user/tokens`                       | `user:read`                   |
//! | DELETE        | `/user/tokens/{id}`                  | `user:write`                  |
//! | POST          | `/admin/users`                       | admin                         |
//! | POST          | `/admin/users/{username}/repos`      | admin                         |
//! | GET           | `/admin/unadopted`                   | admin                         |
//! | POST, DELETE  | `/admin/unadopted/{*path}`           | admin                         |
//!
//! Tokens are created on the `/-/settings/tokens` page, not here: a token
//! shouldn't be able to mint more tokens.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use klotho_core::browse::{DEFAULT_PAGE, MAX_PAGE};
use klotho_core::config::Registration;
use klotho_core::{Action, Core, LogQuery, NewUser, Owner, Repo, Scope, StorageReport, TokenInfo};
use klotho_git::{CommitInfo, RepoInfo, TreeEntryInfo};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::auth::{api_actor, require_admin, require_user};
use crate::error::ApiError;

/// The `/api` router: version 1, plus JSON 404s for anything else under `/api`
/// so it never falls through to the web UI's HTML pages (FR-UI-050).
pub fn router() -> Router<AppState> {
    let v1 = Router::new()
        .route("/instance", get(instance))
        .route("/repos/{owner}/{repo}", get(repo).patch(update_repo))
        .route("/repos/{owner}/{repo}/commits", get(commits))
        .route("/repos/{owner}/{repo}/tree", get(tree))
        .route("/repos/{owner}/{repo}/raw", get(raw))
        .route("/users/{username}/repos", get(user_repos))
        .route("/user", get(current_user))
        .route("/user/repos", post(create_own_repo))
        .route("/user/tokens", get(tokens))
        .route("/user/tokens/{id}", delete(revoke_token))
        .route("/admin/users", post(create_user))
        .route("/admin/users/{username}/repos", post(create_repo))
        .route("/admin/unadopted", get(unadopted))
        .route("/admin/unadopted/{*path}", post(adopt).delete(delete_unadopted));
    Router::new().nest("/v1", v1).fallback(|| async { ApiError::not_found() })
}

#[derive(Serialize)]
struct Instance {
    name: &'static str,
    version: &'static str,
    registration: Registration,
}

async fn instance(State(core): State<Core>) -> Json<Instance> {
    Json(Instance { name: "Klotho", version: env!("CARGO_PKG_VERSION"), registration: core.registration() })
}

/// A repository as the API returns it (FR-API-010).
#[derive(Serialize)]
struct RepoResource {
    id: i64,
    owner: Owner,
    name: String,
    full_name: String,
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
            private: repo.private,
            default_branch: info.default_branch,
            empty: info.empty,
            created_at: repo.created_at,
        })
    }
}

#[derive(Deserialize)]
struct RevQuery {
    /// A branch, tag or commit ID; the default branch if missing.
    rev: Option<String>,
    #[serde(default)]
    path: String,
    limit: Option<u32>,
    cursor: Option<String>,
}

async fn repo(
    State(core): State<Core>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
) -> Result<Json<RepoResource>, ApiError> {
    let actor = api_actor(&core, &headers).await?;
    let repo = core.repo_for(&actor, &owner, &name, Action::Read).await?;
    Ok(Json(RepoResource::build(&core, repo).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateRepo {
    private: Option<bool>,
}

async fn update_repo(
    State(core): State<Core>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
    Json(body): Json<UpdateRepo>,
) -> Result<Json<RepoResource>, ApiError> {
    let actor = api_actor(&core, &headers).await?;
    let repo = match body.private {
        Some(private) => core.set_private(&actor, &owner, &name, private).await?,
        None => core.repo_for(&actor, &owner, &name, Action::Admin).await?,
    };
    Ok(Json(RepoResource::build(&core, repo).await?))
}

async fn commits(
    State(core): State<Core>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
    Query(query): Query<RevQuery>,
) -> Result<Json<Vec<CommitInfo>>, ApiError> {
    let actor = api_actor(&core, &headers).await?;
    let repo = core.repo_for(&actor, &owner, &name, Action::Read).await?;
    let log = LogQuery { rev: query.rev, path: query.path, cursor: query.cursor, limit: query.limit };
    Ok(Json(core.commits(&repo, log).await?.items))
}

async fn tree(
    State(core): State<Core>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
    Query(query): Query<RevQuery>,
) -> Result<Json<Vec<TreeEntryInfo>>, ApiError> {
    let actor = api_actor(&core, &headers).await?;
    let repo = core.repo_for(&actor, &owner, &name, Action::Read).await?;
    Ok(Json(
        core.with_git(&repo, move |git| TreeEntryInfo::list(git, query.rev.as_deref(), &query.path)).await?,
    ))
}

async fn raw(
    State(core): State<Core>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
    Query(query): Query<RevQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let actor = api_actor(&core, &headers).await?;
    let repo = core.repo_for(&actor, &owner, &name, Action::Read).await?;
    let data = core
        .with_git(&repo, move |git| klotho_git::read_blob(git, query.rev.as_deref(), &query.path))
        .await?;
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

/// An owner's repositories that the caller can see, one page at a time, with an
/// RFC 8288 `Link` header to the next page (FR-API-020, 021; FR-ACL-011).
async fn user_repos(
    State(core): State<Core>,
    headers: HeaderMap,
    Path(username): Path<String>,
    Query(page): Query<PageQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let actor = api_actor(&core, &headers).await?;
    let limit = page.limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
    let repos = core.list_repos(&actor, &username, page.cursor.as_deref(), limit).await?;

    let mut response_headers = HeaderMap::new();
    if repos.len() == limit as usize
        && let Some(last) = repos.last()
    {
        let cursor = last.name.to_ascii_lowercase();
        let next = core.urls().api(&format!("/users/{username}/repos?limit={limit}&cursor={cursor}"));
        if let Ok(link) = HeaderValue::from_str(&format!("<{next}>; rel=\"next\"")) {
            response_headers.insert(header::LINK, link);
        }
    }
    let mut resources = Vec::with_capacity(repos.len());
    for repo in repos {
        resources.push(RepoResource::build(&core, repo).await?);
    }
    Ok((response_headers, Json(resources)))
}

#[derive(Serialize)]
struct UserResource {
    id: i64,
    username: String,
    is_admin: bool,
}

async fn current_user(State(core): State<Core>, headers: HeaderMap) -> Result<Json<UserResource>, ApiError> {
    let actor = api_actor(&core, &headers).await?;
    let user = require_user(&actor, Scope::UserRead)?;
    Ok(Json(UserResource { id: user.id, username: user.username.clone(), is_admin: user.is_admin }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateRepo {
    name: String,
    #[serde(default)]
    private: bool,
}

/// Creates a repository owned by the caller.
async fn create_own_repo(
    State(core): State<Core>,
    headers: HeaderMap,
    Json(body): Json<CreateRepo>,
) -> Result<(StatusCode, Json<RepoResource>), ApiError> {
    let actor = api_actor(&core, &headers).await?;
    let user = require_user(&actor, Scope::RepoAdmin)?;
    let repo = core.create_repo_as(&actor, &user.username, &body.name, body.private).await?;
    Ok((StatusCode::CREATED, Json(RepoResource::build(&core, repo).await?)))
}

async fn tokens(State(core): State<Core>, headers: HeaderMap) -> Result<Json<Vec<TokenInfo>>, ApiError> {
    let actor = api_actor(&core, &headers).await?;
    let user = require_user(&actor, Scope::UserRead)?;
    Ok(Json(core.list_tokens(user.id).await?))
}

async fn revoke_token(
    State(core): State<Core>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let actor = api_actor(&core, &headers).await?;
    let user = require_user(&actor, Scope::UserWrite)?;
    core.revoke_token(user.id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateUser {
    username: String,
    email: Option<String>,
    password: Option<String>,
    #[serde(default)]
    admin: bool,
}

async fn create_user(
    State(core): State<Core>,
    headers: HeaderMap,
    Json(body): Json<CreateUser>,
) -> Result<(StatusCode, Json<UserResource>), ApiError> {
    require_admin(&api_actor(&core, &headers).await?)?;
    let new = NewUser {
        username: &body.username,
        email: body.email.as_deref(),
        password: body.password.as_deref(),
        admin: body.admin,
    };
    let user = core.create_user(new).await?;
    Ok((
        StatusCode::CREATED,
        Json(UserResource { id: user.id, username: user.username, is_admin: user.is_admin }),
    ))
}

/// Creates a repository owned by `username`, as Gitea's
/// `POST /admin/users/{username}/repos` does.
async fn create_repo(
    State(core): State<Core>,
    headers: HeaderMap,
    Path(username): Path<String>,
    Json(body): Json<CreateRepo>,
) -> Result<(StatusCode, Json<RepoResource>), ApiError> {
    let actor = api_actor(&core, &headers).await?;
    require_admin(&actor)?;
    let repo = core.create_repo(&username, &body.name, body.private).await?;
    Ok((StatusCode::CREATED, Json(RepoResource::build(&core, repo).await?)))
}

#[derive(Serialize)]
struct StorageReportResource {
    unadopted: Vec<String>,
    missing: Vec<String>,
}

async fn unadopted(
    State(core): State<Core>,
    headers: HeaderMap,
) -> Result<Json<StorageReportResource>, ApiError> {
    require_admin(&api_actor(&core, &headers).await?)?;
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
    headers: HeaderMap,
    Path(path): Path<String>,
    Json(body): Json<Adopt>,
) -> Result<(StatusCode, Json<RepoResource>), ApiError> {
    require_admin(&api_actor(&core, &headers).await?)?;
    let repo = core.adopt(&path, &body.owner, body.name.as_deref()).await?;
    Ok((StatusCode::CREATED, Json(RepoResource::build(&core, repo).await?)))
}

async fn delete_unadopted(
    State(core): State<Core>,
    headers: HeaderMap,
    Path(path): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_admin(&api_actor(&core, &headers).await?)?;
    core.delete_unadopted(&path).await?;
    Ok(StatusCode::NO_CONTENT)
}
