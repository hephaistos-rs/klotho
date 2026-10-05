# API requirements

**Scope:** the conventions of the HTTP JSON API: versioning, resource shapes, pagination, errors, time formats, discoverability and compatibility. Individual API *features* are required by their own subject files; this file only says how every endpoint must behave.

**Not in scope:** how API callers authenticate ([auth.md](auth.md)), rate limits and content-safety headers ([security.md](security.md)), and latency targets ([performance.md](performance.md)).

## Structure

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-API-001 | All JSON API endpoints **must** live under a versioned prefix, `/api/v1/…`. | Gitea/Forgejo use `/api/v1` and GitLab `/api/v4`. Versioning allows breaking changes without breaking existing clients. | must-have |
| FR-API-002 | Within a major version, changes **must** be backward compatible: fields and endpoints may be added but not removed or retyped. Deprecated endpoints **should** send a `Deprecation` header for at least one minor release before removal in the next major version. | GitHub and GitLab API deprecation policies. Lachesis, Atropos and third-party tools depend on the API staying stable. | must-have |
| FR-API-003 | Repository endpoints **must** be addressed as `/api/v1/repos/{owner}/{repo}/…`. | Gitea/Forgejo and GitHub (`/repos/{owner}/{repo}`) use the same shape, so client code can be reused. | must-have |
| FR-API-004 | Every action available in the web UI **should** also be available through the API. Account security actions (passwords, passkeys, second factors, email verification, sessions) are excepted and **may** be UI-only. | Gitea and GitHub make the whole platform scriptable this way. The UI is server-rendered and calls `klotho-core` directly ([ADR 0002](../decisions/0002-topcoat-ui.md)), so parity is kept by a working rule: every UI action goes through a public `klotho-core` service, and the API endpoint ships in the same PR. Credential management through a bearer token would let a stolen token take over the account, and GitHub and Gitea don't offer it either. | should-have |
| FR-API-005 | A machine-readable OpenAPI 3 description of the API **should** be served by the instance and published with each release. | Gitea/Forgejo serve Swagger/OpenAPI; GitHub publishes OpenAPI. It lets clients be generated. | should-have |

## Resources

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-API-010 | Repository resources **must** include `id`, `owner`, `name` (display name), `full_name`, `private`, `default_branch`, `html_url`, `clone_url` (HTTPS) and `ssh_url`. | Every field here appears in GitHub's and Gitea's repository object. Clients need URLs without having to build them from strings. | must-have |
| FR-API-011 | Timestamps **must** be RFC 3339 strings. Commit timestamps **must** keep the author's and committer's original UTC offset. | GitHub, GitLab and Gitea all use ISO 8601/RFC 3339. Keeping the offset preserves information git stores. | must-have |
| FR-API-012 | Object IDs **must** be full hexadecimal strings, never abbreviated. | All four return full SHAs. Abbreviations can become ambiguous as a repository grows. | must-have |

## Collections

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-API-020 | Every endpoint that returns a collection **must** be paginated, with a default page size of 30 and a maximum of 100. | GitHub (30/100), Gitea (`page`/`limit`). No request should be able to return millions of items. | must-have |
| FR-API-021 | Paginated responses **must** include an RFC 8288 `Link` header with `next` (and `prev`, `first` where they can be computed cheaply). They **should** use opaque cursors for collections that change while being paged through. | GitHub uses `Link` headers; Gitea adds `X-Total-Count`. Cursors avoid skipped or duplicated items. | must-have |
| FR-API-022 | Collection endpoints **should** support sorting and filtering on their main fields, such as repositories by owner, visibility and last update. | All four. | should-have |

## Errors

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-API-030 | Error responses **must** use appropriate HTTP status codes and a JSON body with a stable machine-readable `code` (e.g. `repo_not_found`, `name_invalid`) and a human-readable `message`. | GitHub, GitLab and Gitea return JSON errors. A stable code lets clients branch without parsing messages, which can change wording. | must-have |
| FR-API-031 | Validation errors **should** say which field failed and why. | GitHub's `errors[]` array with `field`/`code`; GitLab's per-field messages. | should-have |

## Caching

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-API-040 | `GET` responses for immutable content, such as trees and blobs addressed by object ID, **should** carry long-lived `Cache-Control` and an `ETag`. Other `GET`s **may** support conditional requests with `ETag`/`If-None-Match`. | GitHub supports conditional requests, and 304 responses don't count against its rate limit. It cuts load from polling clients such as CI. | nice-to-have |

## Conflicts with the current implementation

Checked 2026-10-02, during Phase 3.

| Requirement | Current behaviour | Where |
|---|---|---|
| FR-API-020, FR-API-021 (pagination) | **Met for every list so far** (2026-10-03, browse step). `GET /users/{username}/repos`, `…/branches`, `…/tags` and `…/commits` are paged with cursors (max 100) and an RFC 8288 `Link` header. A directory listing (`…/contents`) carries its `next` cursor in the body, because the body is an object, not a list. | [api.rs](../../crates/server/src/api.rs) (`next_link`) |

Met today: FR-API-001 (`/api/v1`, with JSON 404s for anything else under `/api`), FR-API-003 (`/api/v1/repos/{owner}/{repo}`), FR-API-010 (repository resource with ID, owner, display name, URLs built from `server.public_url`), FR-API-011 (RFC 3339 times; commit author and committer dates keep their original UTC offset), FR-API-012 (full object IDs) and FR-API-030 (`{"code", "message"}` errors). Internal error details are hidden from clients; that requirement lives in [security.md](security.md) as NFR-SEC-030.
