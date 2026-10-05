# API endpoints

The full URL map for Klotho: the JSON API, browser-only auth endpoints, the OAuth2 provider, git transport and the UI pages. The conventions (versioning, pagination, errors, timestamps) are set by [requirements/api.md](../requirements/api.md) and aren't repeated here. The paths follow Gitea's `/api/v1`, which itself mirrors GitHub's REST API, so client code and habits carry over.

**Phase** refers to the phases in [ROADMAP.md](../../ROADMAP.md).

## URL spaces

The server's router picks the first matching space, top to bottom:

| Space | Prefix | Who calls it | Auth |
|---|---|---|---|
| JSON API | `/api/v1/…` | CLIs, CI, integrations | `Authorization: Bearer` token only. The session cookie is ignored here ([ADR 0002](../decisions/0002-topcoat-ui.md)) |
| Browser endpoints | `/-/login`, `/-/auth/…`, `/-/settings/…`, `/-/oauth/…` | Browsers: forms, redirects, the passkey script | Session cookie |
| Well-known | `/.well-known/…` | OIDC clients | None |
| Operations | `/-/health`, `/-/ready`, `/-/metrics` | Orchestrators, Prometheus | None, or a metrics token |
| Assets | `/-/assets/…` (embedded, hashed), `/favicon.ico`, `/robots.txt` | Browsers | None |
| Git transport | `/{owner}/{repo}[.git]/info/refs`, `/git-upload-pack`, `/git-receive-pack`, `/info/lfs/…` | git clients | HTTP Basic with a token |
| UI pages | everything else, including Topcoat's own `/_topcoat/…` runtime routes | Browsers | Session cookie. Each page runs `authorize()` on the server |

**Why browser endpoints aren't under `/api/v1`.** Their URLs get registered with identity providers (OIDC callback URLs) or sent in emails (magic links). They must stay the same when the API moves to v2, so they're unversioned and live under the reserved `/-/` prefix (FR-NAME-034).

## Access levels used below

| Level | Meaning |
|---|---|
| **anon** | No credentials needed. Private resources still 404 without access (FR-ACL-013). |
| **user** | Any signed-in user. |
| **read** / **write** / **admin** | That level on the repository (FR-ACL-001). |
| **org-owner** | Owner of the organisation. |
| **site-admin** | Instance administrator. |
| **sudo** | The same level, plus a re-authentication in the last 15 minutes (FR-AUTH-044). |

## Path parameters that can contain `/`

Branch names (`feature/login`), tags and file paths can all contain `/`. Rules:

- **File paths** are always the *last* path segment, as a wildcard: `/contents/{*path}`. This is axum's `{*path}` syntax.
- **Refs** are always passed as a **query parameter**, `?ref=feature/login`, never as a path segment. GitHub and Gitea do the same for contents endpoints.
- **Branch and tag names** in their own endpoints use a trailing wildcard: `/branches/{*name}`.
- For UI URLs like `/{owner}/{repo}/-/tree/feature/login/src/main.rs`, it's ambiguous where the ref ends and the path starts. The page asks the `klotho-core` resolver directly. It tries the longest matching branch or tag first, as GitHub does, and returns `ref = feature/login`, `path = src/main.rs`. API clients get the same resolver through `GET /api/v1/repos/{owner}/{repo}/resolve?spec=…`.

## JSON API: `/api/v1`

### Instance

| Method | Path | Access | Purpose | Req | Phase |
|---|---|---|---|---|---|
| GET | `/version` | anon | Server version and minimum supported API version | FR-API-002 | P3 |
| GET | `/instance` | anon | Public settings the login page needs: instance name, registration mode, enabled sign-in methods (password, passkey, magic link, and the list of OIDC providers) | FR-AUTH-002 | P2 |
| GET | `/openapi.json` | anon | The OpenAPI 3 document | FR-API-005 | P3 |
| POST | `/markdown` | user | Render Markdown to sanitised HTML, e.g. for comment previews. The body can name a repository so `#12` links resolve | NFR-SEC-011 | P3 |

### Auth

There are no sign-in endpoints in `/api/v1`. The API takes only bearer tokens, and signing in happens through the browser routes under [Sign-in and account security](#sign-in-and-account-security-browser-only).

### Current user: `/user`

| Method | Path | Access | Purpose | Req | Phase |
|---|---|---|---|---|---|
| GET, PATCH | `/user` | user | Profile: display name, avatar, preferences | — | P2 |
| GET | `/user/emails` | user | List email addresses (managed in the browser only) | FR-AUTH-004 | P5 |
| GET · DELETE | `/user/tokens` · `/user/tokens/{id}` | user | List and revoke personal access tokens. Creating a token is browser-only, so a stolen token can't mint more | FR-AUTH-011 | P2 |
| GET, POST | `/user/keys` · DELETE `/user/keys/{id}` | user / sudo | SSH public keys | FR-AUTH-014 | P4 |
| GET | `/user/repos` | user | Repositories the user can access | FR-REPO-040 | P3 |
| POST | `/user/repos` | user | Create a repository owned by the user. Needs a signed-in user, so it arrives with accounts; until then `POST /admin/users/{username}/repos` creates repositories | FR-REPO-001 | P2 |
| GET, POST | `/user/applications/oauth2` · PATCH, DELETE `/…/{id}` | user / sudo | OAuth2 apps the user registered | FR-AUTH-035 | P8 |

### Users and organisations

| Method | Path | Access | Purpose | Req | Phase |
|---|---|---|---|---|---|
| GET | `/users/{username}` | anon | Public profile | FR-UI-023 | P3 |
| GET | `/users/{username}/repos?limit=&cursor=` | anon | The user's public repositories, paged with a `Link` header | FR-UI-023, FR-API-020 | P1 |
| POST | `/orgs` | user | Create an organisation | FR-ACL-005 | P7 |
| GET, PATCH, DELETE | `/orgs/{org}` | anon / org-owner / org-owner + sudo | View, edit, delete | FR-ACL-005 | P7 |
| GET, POST | `/orgs/{org}/repos` | anon / member with create rights | List or create the org's repositories | FR-REPO-001 | P7 |
| GET · PUT, DELETE | `/orgs/{org}/members` · `/orgs/{org}/members/{user}` | member / org-owner | Membership | FR-ACL-005 | P7 |
| GET, POST | `/orgs/{org}/teams` | member / org-owner | Teams | FR-ACL-005 | P7 |
| GET, PATCH, DELETE | `/teams/{id}` | member / org-owner | Team settings | FR-ACL-005 | P7 |
| PUT, DELETE | `/teams/{id}/members/{user}` · `/teams/{id}/repos/{owner}/{repo}` | org-owner | Team members and repository grants | FR-ACL-005 | P7 |

### Repositories

`{o}/{r}` = `{owner}/{repo}`, resolved case-insensitively (FR-NAME-021), with rename redirects (FR-NAME-050).

| Method | Path | Access | Purpose | Req | Phase |
|---|---|---|---|---|---|
| GET | `/repos/search?q=&owner=&visibility=&sort=&cursor=` | anon | Search visible repositories | FR-UI-020 | P3 |
| GET | `/repos/{o}/{r}` | read | Repository resource (FR-API-010 fields) | FR-API-010 | P1 |
| PATCH | `/repos/{o}/{r}` | admin | Rename, description, website, topics, visibility, default branch, archive | FR-REPO-010…022 | P6 |
| DELETE | `/repos/{o}/{r}` | admin + sudo | Delete; soft-deleted and restorable | FR-REPO-023/024 | P6 |
| POST | `/repos/{o}/{r}/transfer` | admin + sudo | Transfer to another owner | FR-REPO-021 | P6 |
| GET, POST | `/repos/{o}/{r}/forks` | read / user | List forks, fork | FR-REPO-030 | P6 |
| POST | `/repos/migrate` | user | Import from a URL, optionally as a mirror | FR-REPO-031/033 | P6 |
| POST | `/repos/{o}/{r}/mirror-sync` | write | Sync a pull mirror now | FR-REPO-031 | P6 |
| GET · DELETE | `/repos/{o}/{r}/redirects` · `/repos/{o}/{r}/redirects/{name}` | admin | Old names that point here | FR-NAME-056 | P6 |
| GET · PUT, DELETE | `/repos/{o}/{r}/collaborators` · `/…/collaborators/{user}` | read / admin | Direct access grants | FR-ACL-006 | P7 |
| GET, POST · DELETE | `/repos/{o}/{r}/keys` · `/…/keys/{id}` | admin | Deploy keys | FR-AUTH-015 | P4 |
| GET, POST · PATCH, DELETE | `/repos/{o}/{r}/branch-protections` · `/…/{*pattern}` | admin | Protected branches | FR-ACL-040/041 | P7 |

### Git data (read-only)

All accept `?ref=` (default: the default branch). Collections are paginated with cursors (FR-API-020/021).

| Method | Path | Access | Purpose | Req | Phase |
|---|---|---|---|---|---|
| GET | `/repos/{o}/{r}/resolve?spec=` | read | Split `ref/path` from a UI URL | — | P3 |
| GET | `/repos/{o}/{r}/branches` · `/branches/{*name}` | read | Branches with their latest commit | FR-UI-007 | P3 |
| GET | `/repos/{o}/{r}/tags` · `/tags/{*name}` | read | Tags | FR-UI-007 | P3 |
| GET | `/repos/{o}/{r}/commits?ref=&path=&cursor=` | read | Commit log | FR-UI-005 | P3 |
| GET | `/repos/{o}/{r}/commits/{sha}` | read | One commit with its diff stats | FR-UI-005 | P3 |
| GET | `/repos/{o}/{r}/commits/{sha}/diff` | read | The full diff, paginated per file for large commits | FR-UI-005 | P3 |
| GET | `/repos/{o}/{r}/compare/{base}...{head}` | read | Commits and diff between two refs (URL-encode any `/` in the refs) | FR-UI-008 | P6 |
| GET | `/repos/{o}/{r}/contents` · `/contents/{*path}?ref=` | read | A directory listing, or file metadata plus content for small text files | FR-UI-001 | P3 |
| GET | `/repos/{o}/{r}/raw/{*path}?ref=` | read | The file's bytes, streamed, as `application/octet-stream` with `nosniff` | NFR-PERF-013, NFR-SEC-010 | P3 |
| GET | `/repos/{o}/{r}/readme?ref=&dir=` | read | The README of the root (or of `dir`) rendered to sanitised HTML, with relative links pointing at the repository's pages | FR-UI-002 | P3 |
| GET | `/repos/{o}/{r}/blame/{*path}?ref=` | read | Blame | FR-UI-006 | P6 |
| GET | `/repos/{o}/{r}/archive?ref=&format=zip\|tar.gz` | read | Download an archive (streamed) | FR-GIT-012 | P4 |
| GET | `/repos/{o}/{r}/graph?ref=&cursor=` | read | Up to 50 commits with parents and ref labels, for the gitGraph | FR-UI-041/042 | P8 |
| GET | `/repos/{o}/{r}/thread/{sha}` | read | Thread view data: push event, statuses, containing tags and releases, deployments | FR-UI-040 | P8 |

### Integrations

| Method | Path | Access | Purpose | Req | Phase |
|---|---|---|---|---|---|
| GET, POST | `/repos/{o}/{r}/hooks` | admin | Webhooks | FR-INT-001 | P8 |
| GET, PATCH, DELETE | `/repos/{o}/{r}/hooks/{id}` | admin | Edit or delete a webhook | FR-INT-001 | P8 |
| GET | `/repos/{o}/{r}/hooks/{id}/deliveries` | admin | Delivery log | FR-INT-005 | P8 |
| POST | `/repos/{o}/{r}/hooks/{id}/deliveries/{delivery}/redeliver` | admin | Send a delivery again | FR-INT-005 | P8 |
| POST | `/repos/{o}/{r}/statuses/{sha}` | write, or a token with the `status` scope | Report a CI status (used by Lachesis) | FR-INT-010 | P8 |
| GET | `/repos/{o}/{r}/commits/{sha}/status` | read | Combined status | FR-INT-011 | P8 |
| GET, POST | `/repos/{o}/{r}/deployments` · POST `/…/deployments/{id}/statuses` | read / write | Deployments (used by Atropos) | FR-INT-013 | P8 |
| GET, POST · GET, PATCH, DELETE | `/repos/{o}/{r}/releases` · `/…/releases/{id}` | read / write | Releases | FR-COLLAB-030 | P9 |
| POST | `/repos/{o}/{r}/releases/{id}/assets` | write | Upload a release asset | FR-COLLAB-030 | P9 |

### Collaboration (P9, outline)

These follow Gitea's shapes, and are specified in detail when P9 starts:

- `/repos/{o}/{r}/issues`, `/issues/{n}`, `/issues/{n}/comments`
- `/repos/{o}/{r}/pulls`, `/pulls/{n}`, `/pulls/{n}/merge`, `/pulls/{n}/reviews`, `/pulls/{n}/comments`
- `/repos/{o}/{r}/labels`, `/milestones`
- `/notifications`, `/repos/{o}/{r}/subscription`

Issues and pull requests share one number sequence (FR-COLLAB-021).

### Administration: `/admin` (site-admin)

| Method | Path | Purpose | Req | Phase |
|---|---|---|---|---|
| POST | `/admin/users` | Create a user (`{"username"}`). Phase 2 adds listing, passwords and emails | FR-AUTH-001 | P1 |
| GET | `/admin/users` | List users | FR-AUTH-001 | P2 |
| POST | `/admin/users/{username}/repos` | Create a repository for a user (as in Gitea) | FR-REPO-001 | P1 |
| PATCH, DELETE | `/admin/users/{username}` | Suspend, promote, delete (sudo) | FR-ACL-004 | P2 |
| GET | `/admin/unadopted` | Directories under storage that aren't known repositories | FR-STOR-020 | P1 |
| POST, DELETE | `/admin/unadopted/{dir}` | Adopt or delete one | FR-REPO-035 | P1 |
| GET | `/admin/audit-log?cursor=` | Audit events | NFR-SEC-050 | P5 |
| GET, POST | `/admin/hooks` | System webhooks | FR-INT-007 | P8 |
| GET, POST, PATCH, DELETE | `/admin/auth-providers` | Configure OIDC providers | FR-AUTH-030 | P5 |

## Sign-in and account security (browser only)

Topcoat routes in `klotho-web`. They are HTML pages and form `POST`s that answer with redirects, except the two passkey endpoints, which the passkey script calls with JSON. They're unversioned under `/-/` so links in emails stay valid (FR-NAME-034). All of them are excepted from API parity (FR-API-004).

| Method | Path | Access | Purpose | Req | Phase |
|---|---|---|---|---|---|
| GET, POST | `/-/register` | anon | Create an account (if registration is open, or with an invite code) | FR-AUTH-001/002 | P2 |
| GET, POST | `/-/login?return_to=` | anon | Sign-in page; the form posts username or email + password. Redirects to `return_to`, or to `/-/login/mfa` with a ticket | FR-AUTH-010 | P2 |
| GET, POST | `/-/login/mfa` | anon + ticket | Second factor: TOTP code, recovery code or security key | FR-AUTH-032/033 | P5 |
| POST | `/-/auth/passkey/options` · `/-/auth/passkey/verify` | anon | Passwordless passkey sign-in (JSON, same-origin only) | FR-AUTH-016 | P5 |
| POST | `/-/auth/magic-link` | anon | Email a sign-in link. The same "check your email" page whether or not the address exists | FR-AUTH-006 | P5 |
| GET · POST | `/-/auth/magic?token=…` | anon | `GET` shows a "Sign in as …?" page and doesn't use the token; its button `POST`s the token to sign in | FR-AUTH-006 | P5 |
| GET, POST | `/-/auth/forgot` · `/-/auth/reset?token=…` · `/-/auth/verify?token=…` | anon | Password reset and email verification (same confirm-then-`POST` pattern) | FR-AUTH-004/005 | P5 |
| GET, POST | `/-/sudo?return_to=` | user | Re-authenticate (password, passkey or TOTP) to enter sudo mode | FR-AUTH-044 | P5 |
| POST | `/-/logout` | user | End the current session | — | P2 |
| POST | `/-/theme` | anon | Store the colour scheme (system, light or dark) in a cookie and go back to `return_to`. A browser display preference, not account data, so it has no API endpoint either ([ADR 0005](../decisions/0005-tailwind-and-design-system.md)) | NFR-UI-005 | P3 |
| GET, POST | `/-/settings/password` | user + current password (P2); sudo from P5 | Change the password, or remove it if a passkey exists (P5) | FR-AUTH-010/016 | P2 |
| GET, POST | `/-/settings/tokens` | user / sudo | Create a personal access token (shown once) | FR-AUTH-011 | P2 |
| GET, POST | `/-/settings/emails` | user / sudo | Add, remove, verify and make primary | FR-AUTH-004 | P5 |
| GET, POST | `/-/settings/sessions` | user | List and revoke sessions | FR-AUTH-042 | P5 |
| GET, POST | `/-/settings/security` | sudo | TOTP enrolment, recovery codes, passkeys (`…/passkeys/options` for registration), linked SSO identities | FR-AUTH-016/032/033/036 | P5 |

**Tokens in links.** `?token=` puts the token in the request line, so the tracing layer **must** redact query strings on `/-/auth/*`, and these pages set `Referrer-Policy: no-referrer`. A `GET` never uses up a token, so mail scanners that prefetch links are harmless.

## Browser endpoints: `/-/auth/oidc`, `/-/oauth`

These answer with redirects or HTML pages. They are not JSON.

| Method | Path | Purpose | Req | Phase |
|---|---|---|---|---|
| GET | `/-/auth/oidc/{provider}/start?return_to=` | Redirect to the identity provider (sets PKCE, state and nonce) | FR-AUTH-037 | P5 |
| GET | `/-/auth/oidc/{provider}/callback` | Handle the provider's response, create a session or MFA ticket, then redirect to `return_to` | FR-AUTH-030/036 | P5 |
| GET | `/-/auth/oidc/{provider}/link` | Link a new SSO identity to this account (sudo) | FR-AUTH-036 | P5 |
| GET | `/-/oauth/authorize` | Consent page for third-party apps | FR-AUTH-035 | P8 |
| POST | `/-/oauth/authorize` | Consent decision; redirects to the app with a code | FR-AUTH-035 | P8 |
| POST | `/-/oauth/token` | Exchange the code or refresh token (PKCE required) | FR-AUTH-035 | P8 |
| GET | `/-/oauth/userinfo` | OIDC userinfo | FR-AUTH-035 | P8 |
| GET | `/-/oauth/jwks` | Public signing keys | FR-AUTH-035 | P8 |
| POST | `/-/oauth/revoke` | Revoke a token (RFC 7009) | FR-AUTH-035 | P8 |
| POST | `/-/oauth/device/code` · GET `/-/device` | Device authorization grant, and the page where the user enters the code | FR-AUTH-024 | P8 |
| GET | `/.well-known/openid-configuration` | OIDC discovery | FR-AUTH-035 | P8 |

## Operations

| Method | Path | Access | Purpose | Req | Phase |
|---|---|---|---|---|---|
| GET | `/-/health` | anon | Liveness | NFR-OPS-022 | P0 |
| GET | `/-/ready` | anon | Readiness (database and storage reachable) | NFR-OPS-022 | P1 |
| GET | `/-/metrics` | metrics token | Prometheus metrics | NFR-OPS-021 | P10 |

## Git transport

`.git` is optional in every path (FR-NAME-040). Authentication is HTTP Basic with username + token (FR-AUTH-020), with a `401` challenge when credentials are needed (FR-AUTH-023).

| Method | Path | Purpose | Req | Phase |
|---|---|---|---|---|
| GET | `/{o}/{r}.git/info/refs?service=git-upload-pack\|git-receive-pack` | Ref advertisement | FR-GIT-001 | exists (without owner) |
| POST | `/{o}/{r}.git/git-upload-pack` | Clone and fetch | FR-GIT-001 | exists (without owner) |
| POST | `/{o}/{r}.git/git-receive-pack` | Push | FR-GIT-001 | exists (without owner) |
| POST | `/{o}/{r}.git/info/lfs/objects/batch` | LFS batch API | FR-GIT-030 | P10 |
| GET, PUT | `/{o}/{r}.git/info/lfs/objects/{oid}` | LFS basic transfer (the `href` returned by the batch API) | FR-GIT-030 | P10 |
| POST, GET | `/{o}/{r}.git/info/lfs/locks…` | LFS file locking | FR-GIT-030 | nice-to-have |

SSH uses the same repository paths: `git@host:{owner}/{repo}.git` (FR-GIT-002, phase P4).

## UI page routes

Rendered on the server by `klotho-web` (FR-UI-050), each with its own title and meta tags, and status 404 when not visible (FR-UI-051). The routes follow the module tree in `crates/web/src/app/` (Topcoat's module-based routing).

| Route | Page |
|---|---|
| `/` | Dashboard when signed in, otherwise explore |
| `/-/login`, `/-/register`, `/-/explore`, `/-/settings/…`, `/-/admin/…`, `/-/new` | Instance pages |
| `/{owner}` | Profile or organisation page |
| `/{owner}/{repo}` | Repository home (README) |
| `/{owner}/{repo}/-/tree/{*spec}`, `/-/blob/{*spec}`, `/-/raw/{*spec}` | Tree, file and raw download (`spec` = ref + path, split by the resolver). `raw` is the browser's download: it uses the session, where the API's `raw` uses a token |
| `/{owner}/{repo}/-/commits/{*spec}`, `/-/commit/{sha}` | History and commit (with the thread view). `-/commits` alone redirects to the default branch |
| `/{owner}/{repo}/-/branches`, `/-/tags`, `/-/compare/…`, `/-/settings/…` | Other repository pages |

## Migrating the existing endpoints

| Today | Becomes | Changes |
|---|---|---|
| `GET /api/repos` | `GET /api/v1/users/{username}/repos` (P1); `GET /api/v1/user/repos`, `GET /api/v1/repos/search` (P2, P3) | Paginated; filtered to what the caller can see |
| `POST /api/repos` `{name}` | `POST /api/v1/admin/users/{username}/repos` `{name}` (P1); `POST /api/v1/user/repos` `{name, private, description}` (P2) | Requires authentication from P2; rejects `.git` names |
| `GET /api/repos/{repo}` | `GET /api/v1/repos/{o}/{r}` | Adds ID, owner, display name and URLs |
| `GET /api/repos/{repo}/commits?rev=&limit=` | `GET /api/v1/repos/{o}/{r}/commits?ref=&cursor=` | `rev` → `ref`; cursor pagination; RFC 3339 times |
| `GET /api/repos/{repo}/tree?rev=&path=` | `GET /api/v1/repos/{o}/{r}/contents/{*path}?ref=` | Path moves into the URL |
| `GET /api/repos/{repo}/raw?rev=&path=` | `GET /api/v1/repos/{o}/{r}/raw/{*path}?ref=` | Streamed; adds `nosniff` |
| `/{repo}/info/refs` etc. | `/{o}/{r}/info/refs` etc. | Owner added; authentication added |

**Status after Phase 1.** The API is at `/api/v1` and owner-scoped, and git transport is at `/{o}/{r}`. The three browsing endpoints moved as they were, keeping their old query parameters for now: `/api/v1/repos/{o}/{r}/commits?rev=&limit=`, `/tree?rev=&path=` and `/raw?rev=&path=` (`raw` already sends `nosniff`). Phase 3 replaces them with the shapes above.

**Status during Phase 3.** `klotho-core` now has the read services behind the P3 rows. Until the endpoints above exist, the old ones call them: `/commits?rev=&path=&cursor=&limit=` returns the new commit shape (RFC 3339 dates with their offset) but no next cursor; `/tree?rev=&path=&cursor=&limit=` returns the `Contents` shape, a paged directory listing with `next` in the body, or a file, symlink or submodule; and `/raw?rev=&path=` streams, with `Content-Length` and `nosniff`. `contents/{*path}?ref=`, `raw/{*path}?ref=`, `readme`, `resolve`, `branches` and `tags` are still to come.

**Status after the browse step (2026-10-03).** The interim `/tree` and `/raw?path=` are gone. `resolve`, `branches` (and `/branches/{*name}`), `tags` (and `/tags/{*name}`), `commits?ref=&path=&cursor=`, `contents` and `contents/{*path}`, `raw/{*path}` and `readme?ref=&dir=` exist as listed. `branches`, `tags` and `commits` return a JSON array with the next page in an RFC 8288 `Link` header; `contents` keeps `next` in its body. The README's HTML comes from comrak and then `ammonia` (NFR-SEC-011), and its relative links and images point at the web pages (`/-/blob/…`, `/-/raw/…`). The pages that use the same services are `/{owner}`, `/{owner}/{repo}`, `/-/tree`, `/-/blob`, `/-/raw`, `/-/commits`, `/-/branches` and `/-/tags`. Still to come in Phase 3: `commits/{sha}` and its diff, `/user/repos`, `/users/{username}`, search, `/version` and `openapi.json`.
