//! The JSON API, `/api/v1` (docs/design/api-endpoints.md). Every request is
//! authenticated with `Authorization: Bearer <token>` or is anonymous, and every
//! handler goes through `klotho-core`'s permission checks (FR-ACL-050).
//!
//! | Method        | Path                                 | Needs                         |
//! |---------------|--------------------------------------|-------------------------------|
//! | GET           | `/instance`                          | nothing                       |
//! | GET, PATCH    | `/repos/{owner}/{repo}`              | read; admin + `repo:admin`    |
//! | GET           | `/repos/{owner}/{repo}/resolve`      | read (`spec`)                 |
//! | GET           | `/repos/{owner}/{repo}/branches[/{*name}]` | read; `Link`            |
//! | GET           | `/repos/{owner}/{repo}/tags[/{*name}]` | read; `Link`                |
//! | GET           | `/repos/{owner}/{repo}/commits`      | read (`ref`, `path`); `Link`  |
//! | GET           | `/repos/{owner}/{repo}/contents[/{*path}]` | read (`ref`)            |
//! | GET           | `/repos/{owner}/{repo}/raw/{*path}`  | read (`ref`)                  |
//! | GET           | `/repos/{owner}/{repo}/readme`       | read (`ref`, `dir`)           |
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
use klotho_core::browse::{BranchInfo, Contents, Resolved, TagInfo};
use klotho_core::browse::{DEFAULT_PAGE, MAX_PAGE};
use klotho_core::config::Registration;
use klotho_core::{
    Action, Core, LogQuery, NewUser, Owner, Readme, Repo, Scope, StorageReport, TokenInfo, encode_path,
};
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
        .route("/repos/{owner}/{repo}/resolve", get(resolve))
        .route("/repos/{owner}/{repo}/branches", get(branches))
        .route("/repos/{owner}/{repo}/branches/{*name}", get(branch))
        .route("/repos/{owner}/{repo}/tags", get(tags))
        .route("/repos/{owner}/{repo}/tags/{*name}", get(tag))
        .route("/repos/{owner}/{repo}/commits", get(commits))
        .route("/repos/{owner}/{repo}/contents", get(root_contents))
        .route("/repos/{owner}/{repo}/contents/{*path}", get(contents))
        .route("/repos/{owner}/{repo}/raw/{*path}", get(raw))
        .route("/repos/{owner}/{repo}/readme", get(readme))
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
        let info = core.repo_info(&repo).await?;
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

/// The query of the read endpoints. Each uses the fields it needs.
#[derive(Deserialize)]
struct RefQuery {
    /// A branch, tag or commit ID; the default branch if missing.
    #[serde(rename = "ref")]
    rev: Option<String>,
    /// For `/commits`: only commits that changed this path.
    #[serde(default)]
    path: String,
    /// For `/readme`: the directory to look in, the root if empty.
    #[serde(default)]
    dir: String,
    limit: Option<u32>,
    /// From the previous page's `Link: <…>; rel="next"`.
    cursor: Option<String>,
}

/// An RFC 8288 `Link` header to the next page, when there is one
/// (FR-API-020, 021). `path` is under `/api/v1`, `params` the query without
/// the cursor.
fn next_link(core: &Core, path: &str, params: &[(&str, Option<&str>)], next: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    let Some(next) = next else {
        return headers;
    };
    let mut query = String::new();
    for (name, value) in params.iter().copied().chain([("cursor", Some(next))]) {
        if let Some(value) = value {
            query.push(if query.is_empty() { '?' } else { '&' });
            query.push_str(&format!("{name}={}", encode_query(value)));
        }
    }
    let url = core.urls().api(&format!("{}{query}", encode_path(path)));
    if let Ok(link) = HeaderValue::from_str(&format!("<{url}>; rel=\"next\"")) {
        headers.insert(header::LINK, link);
    }
    headers
}

/// [`encode_path`], with `/` and `+` encoded too: a query decoder reads a
/// bare `+` as a space.
fn encode_query(value: &str) -> String {
    encode_path(value).replace('/', "%2F").replace('+', "%2B")
}

async fn resolve(
    State(core): State<Core>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
    Query(query): Query<SpecQuery>,
) -> Result<Json<Resolved>, ApiError> {
    let actor = api_actor(&core, &headers).await?;
    let repo = core.repo_for(&actor, &owner, &name, Action::Read).await?;
    Ok(Json(core.resolve_spec(&repo, &query.spec).await?))
}

#[derive(Deserialize)]
struct SpecQuery {
    /// `ref/path` as in a web URL, e.g. `feature/x/src/lib.rs`.
    spec: String,
}

async fn branches(
    State(core): State<Core>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
    Query(query): Query<RefQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let actor = api_actor(&core, &headers).await?;
    let repo = core.repo_for(&actor, &owner, &name, Action::Read).await?;
    let page = core.branches(&repo, query.cursor.as_deref(), query.limit).await?;
    let limit = query.limit.map(|limit| limit.to_string());
    let link = next_link(
        &core,
        &format!("/repos/{}/branches", repo.full_name()),
        &[("limit", limit.as_deref())],
        page.next.as_deref(),
    );
    Ok((link, Json(page.items)))
}

async fn branch(
    State(core): State<Core>,
    headers: HeaderMap,
    Path((owner, name, branch)): Path<(String, String, String)>,
) -> Result<Json<BranchInfo>, ApiError> {
    let actor = api_actor(&core, &headers).await?;
    let repo = core.repo_for(&actor, &owner, &name, Action::Read).await?;
    Ok(Json(core.branch(&repo, &branch).await?))
}

async fn tags(
    State(core): State<Core>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
    Query(query): Query<RefQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let actor = api_actor(&core, &headers).await?;
    let repo = core.repo_for(&actor, &owner, &name, Action::Read).await?;
    let page = core.tags(&repo, query.cursor.as_deref(), query.limit).await?;
    let limit = query.limit.map(|limit| limit.to_string());
    let link = next_link(
        &core,
        &format!("/repos/{}/tags", repo.full_name()),
        &[("limit", limit.as_deref())],
        page.next.as_deref(),
    );
    Ok((link, Json(page.items)))
}

async fn tag(
    State(core): State<Core>,
    headers: HeaderMap,
    Path((owner, name, tag)): Path<(String, String, String)>,
) -> Result<Json<TagInfo>, ApiError> {
    let actor = api_actor(&core, &headers).await?;
    let repo = core.repo_for(&actor, &owner, &name, Action::Read).await?;
    Ok(Json(core.tag(&repo, &tag).await?))
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

/// The commit log, newest first. The next page's cursor is in the `Link`
/// header; it keeps the page stable while the branch moves (NFR-PERF-014).
async fn commits(
    State(core): State<Core>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
    Query(query): Query<RefQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let actor = api_actor(&core, &headers).await?;
    let repo = core.repo_for(&actor, &owner, &name, Action::Read).await?;
    let log = LogQuery {
        rev: query.rev.clone(),
        path: query.path.clone(),
        cursor: query.cursor,
        limit: query.limit,
    };
    let page = core.commits(&repo, log).await?;
    let limit = query.limit.map(|limit| limit.to_string());
    let path = (!query.path.is_empty()).then_some(query.path.as_str());
    let link = next_link(
        &core,
        &format!("/repos/{}/commits", repo.full_name()),
        &[("ref", query.rev.as_deref()), ("path", path), ("limit", limit.as_deref())],
        page.next.as_deref(),
    );
    Ok((link, Json(page.items)))
}

async fn root_contents(
    state: State<Core>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
    query: Query<RefQuery>,
) -> Result<Json<Contents>, ApiError> {
    contents(state, headers, Path((owner, name, String::new())), query).await
}

/// A directory listing (paged, `next` in the body), or a file with its text if
/// it is small and not binary, a symlink or a submodule (FR-UI-001).
async fn contents(
    State(core): State<Core>,
    headers: HeaderMap,
    Path((owner, name, path)): Path<(String, String, String)>,
    Query(query): Query<RefQuery>,
) -> Result<Json<Contents>, ApiError> {
    let actor = api_actor(&core, &headers).await?;
    let repo = core.repo_for(&actor, &owner, &name, Action::Read).await?;
    let (rev, cursor) = (query.rev.as_deref(), query.cursor.as_deref());
    Ok(Json(core.contents(&repo, rev, &path, cursor, query.limit).await?))
}

/// The README rendered to sanitised HTML (FR-UI-002, NFR-SEC-011), or `404
/// readme_not_found`.
async fn readme(
    State(core): State<Core>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
    Query(query): Query<RefQuery>,
) -> Result<Json<Readme>, ApiError> {
    let actor = api_actor(&core, &headers).await?;
    let repo = core.repo_for(&actor, &owner, &name, Action::Read).await?;
    match core.readme_html(&repo, query.rev.as_deref(), &query.dir).await? {
        Some(readme) => Ok(Json(readme)),
        None => Err(ApiError::new(StatusCode::NOT_FOUND, "readme_not_found", "no README here")),
    }
}

async fn raw(
    State(core): State<Core>,
    headers: HeaderMap,
    Path((owner, name, path)): Path<(String, String, String)>,
    Query(query): Query<RefQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let actor = api_actor(&core, &headers).await?;
    let repo = core.repo_for(&actor, &owner, &name, Action::Read).await?;
    let file = core.raw(&repo, query.rev.as_deref(), &path).await?;
    let chunks = futures_util::stream::unfold(file.chunks, |mut chunks| async move {
        chunks.recv().await.map(|chunk| (chunk, chunks))
    });
    // Always octet-stream with nosniff, so a pushed HTML file can't run as a page
    // on this origin (NFR-SEC-010).
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_owned()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
            (header::CONTENT_LENGTH, file.size.to_string()),
        ],
        axum::body::Body::from_stream(chunks),
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
