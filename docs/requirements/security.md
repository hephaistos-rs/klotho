# Security requirements

**Scope:** platform-wide protections that aren't about identity or permissions. That covers transport security, safe serving of user content, browser hardening, abuse limits, outbound request safety, subprocess safety, secrets at rest, error disclosure, audit logging and the security process.

**Not in scope:** authentication, including login throttling ([auth.md](auth.md)), and authorisation ([access-control.md](access-control.md)).

## Transport

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| NFR-SEC-001 | Klotho **must** be deployable with HTTPS only, either with built-in TLS or behind a reverse proxy it has been told it is behind. It **must** build every generated URL from a configured public base URL, never from the request's `Host` header. | Gitea (`ROOT_URL`) and GitLab (`external_url`). Building URLs from `Host` lets an attacker poison links, for example in password reset emails. | must-have |
| NFR-SEC-002 | When served over HTTPS, responses **should** include `Strict-Transport-Security`. | Standard practice. | should-have |
| NFR-SEC-003 | The server **must** trust `X-Forwarded-*` headers only from configured proxy addresses. | Gitea `REVERSE_PROXY_TRUSTED_PROXIES`. Otherwise clients can spoof their IP address and dodge rate limits and audit logs. | must-have |

## User content

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| NFR-SEC-010 | Raw repository content **must** never be served with a content type that the browser executes on Klotho's main origin. That means `application/octet-stream` or `text/plain` plus `X-Content-Type-Options: nosniff`, or a separate content origin. | GitHub serves raw files from `raw.githubusercontent.com` as `text/plain`. Otherwise a pushed HTML file becomes stored XSS. | must-have |
| NFR-SEC-011 | Rendered Markdown and other markup **must** be sanitised against an allowlist of HTML elements and attributes. | All four sanitise. READMEs are content from untrusted users. | must-have |
| NFR-SEC-012 | HTML pages **should** send a `Content-Security-Policy` that forbids inline scripts and restricts framing (`frame-ancestors 'self'`). | GitHub and GitLab use strict CSP as a second layer against XSS and clickjacking. | should-have |
| NFR-SEC-013 | State-changing requests authenticated by a cookie **must** be protected against CSRF, with a token, a same-site check of `Origin`, or both. | All four. | must-have |
| NFR-SEC-014 | Mermaid **must** run with `securityLevel: 'strict'`. All user-controlled text placed in generated diagram source (commit messages, branch and tag names, status descriptions, usernames) **must** be escaped first, so it can't end a label or add diagram directives. | Commit messages and branch names come from any pusher. Mermaid has had XSS vulnerabilities in the past, and injected directives could change how a diagram renders. | must-have |

## Abuse limits

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| NFR-SEC-020 | Request bodies **must** have configurable size limits: small for JSON API requests (default 1 MiB), larger for git pushes (default 2 GiB) and LFS uploads. Requests over the limit **must** be rejected with `413`. | GitLab (max push size) and Gitea (`LFS_MAX_FILE_SIZE`). Without limits, one request can fill the disk. | should-have |
| NFR-SEC-021 | The API and git endpoints **should** be rate-limited per user and per source IP, with configurable limits. Limited responses **should** return `429` with `Retry-After`. | GitHub (5000 req/h authenticated) and GitLab rate limits. Protects capacity from runaway scripts. | should-have |
| NFR-SEC-022 | The number of concurrent git subprocesses **must** be capped, and extra requests queued or rejected rather than started. | GitLab/Gitaly concurrency limits. Each clone of a large repository can use a lot of CPU and memory. | must-have |
| NFR-SEC-023 | Git subprocesses **should** have a configurable wall-clock timeout, and **must** be killed if the client disconnects. | Gitea times out git operations. Leftover `upload-pack` processes pile up and use resources for nothing. | should-have |

## Error disclosure

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| NFR-SEC-030 | Error responses **must not** reveal internal details: no file paths, stack traces, SQL, or subprocess output unrelated to the user's request. | Standard. | must-have |

## Outbound requests

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| NFR-SEC-031 | Server-side requests to user-supplied URLs (webhooks, mirrors, imports, OIDC discovery) **must** refuse loopback, link-local, private and metadata addresses by default, checking the address after DNS resolution and after every redirect. Administrators **may** allowlist hosts. | Gitea `ALLOWED_HOST_LIST` and GitLab's "Allow requests to the local network" (off by default). Prevents SSRF against internal services. | must-have |

## Subprocesses and secrets

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| NFR-SEC-040 | Subprocesses **must** be started without a shell, with every user-influenced value passed as a separate argument that can't be read as an option. | Several CVEs in git frontends came from argument injection, e.g. `--upload-pack=` smuggled in as a repository name. | must-have |
| NFR-SEC-041 | Git subprocesses **should** get a minimal environment built from an allowlist (`PATH`, `HOME`, `GIT_PROTOCOL`, …), not the server's full environment. | Gitea builds a fixed environment for git commands. Server secrets in environment variables shouldn't leak into hooks or error output. | should-have |
| NFR-SEC-042 | Secrets that Klotho has to be able to read back later, such as webhook secrets, mirror credentials and OAuth client secrets, **must** be encrypted at rest with a key kept outside the database. | GitLab encrypts these with `db_key_base`, Gitea with `SECRET_KEY`. A database dump then doesn't expose third-party credentials. | must-have |

## Audit and process

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| NFR-SEC-050 | Security-relevant events **should** be written to an append-only audit log, with actor, source IP, target and time. The events are: sign-ins, failed sign-ins, token and key creation and deletion, permission changes, repository visibility changes, deletion and transfer, force-pushes to protected branches, and admin actions. | GitHub and GitLab audit logs. Needed for incident investigation. | should-have |
| NFR-SEC-051 | The project **should** publish a `SECURITY.md` with a private reporting channel, and run dependency vulnerability checks (e.g. `cargo audit`/`cargo deny`) in CI. | Gitea, Forgejo and GitLab all have disclosure policies. | should-have |

## Conflicts with the current implementation

Checked against commit `6b4895f`.

| Requirement | Current behaviour | Where |
|---|---|---|
| NFR-SEC-010 (safe raw content) | **Partial.** Raw blobs are served as `application/octet-stream`, which is the right idea, but without `X-Content-Type-Options: nosniff`. | [api.rs:102-111](../../crates/server/src/api.rs#L102-L111) |
| NFR-SEC-020 (body size limits) | **Partial.** `/api` has a configurable limit (default 1 MiB, `413` above it). The body limit is still turned off for git routes with no replacement, so a single push can be as large as the disk allows. | [git_http.rs](../../crates/server/src/git_http.rs) (`router`) |
| NFR-SEC-022, NFR-SEC-023 (subprocess caps and cleanup) | **Conflict.** Each git request spawns a `git` process with no concurrency cap and no timeout. The child isn't created with `kill_on_drop`, so a client that disconnects mid-clone leaves the process running until git notices the broken pipe. | [git_http.rs:120-165](../../crates/server/src/git_http.rs#L120-L165) |
| NFR-SEC-041 (minimal git environment) | **Conflict.** `git` inherits the server's entire environment. Only `GIT_PROTOCOL` is set explicitly, though it is correctly allowlisted. | [git_http.rs:62-73](../../crates/server/src/git_http.rs#L62-L73) |

Met today: NFR-SEC-030 (500 responses say only "internal server error" and the details are logged; [error.rs:28-36](../../crates/server/src/error.rs#L28-L36)) and NFR-SEC-040 (git is called with separate arguments and an absolute path that starts with no `-`).
