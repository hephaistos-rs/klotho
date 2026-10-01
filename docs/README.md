# Klotho documentation

## Research

| Document | Contents |
|---|---|
| [research/forge-comparison.md](research/forge-comparison.md) | How GitHub, GitLab, Gitea and Forgejo handle repository naming: case, characters, reserved names, the `.git` suffix, renames and redirects, and on-disk layout. Covers where they agree and differ, and what Klotho should borrow. |

## Decisions

| Document | Decision |
|---|---|
| [decisions/0001-stack.md](decisions/0001-stack.md) | Technology stack: gix plus the `git` program, axum, sqlx (its SvelteKit frontend is superseded by ADR 0002) |
| [decisions/0002-topcoat-ui.md](decisions/0002-topcoat-ui.md) | Web UI: server-rendered with Topcoat, mounted inside axum, assets embedded in the binary |
| [decisions/0003-data-and-file-storage.md](decisions/0003-data-and-file-storage.md) | One data directory; git repositories on local disk; other files (LFS, releases, attachments) local or in S3 |

## Design

| Document | Contents |
|---|---|
| [design/api-endpoints.md](design/api-endpoints.md) | Every URL Klotho serves (JSON API, browser auth endpoints, OAuth2 provider, git transport, page routes), with access levels, requirement IDs and roadmap phase |
| [design/auth-flows.md](design/auth-flows.md) | Credential data model, sessions and CSRF, password, passkey, magic link and OIDC sign-in, 2FA, sudo mode, tokens, git HTTP and SSH authentication |

The build order and package list are in [ROADMAP.md](../ROADMAP.md) at the repository root.

## Requirements

These are the baseline Klotho is built against. Each file covers one subject, and every requirement lives in exactly one file. Files point to each other by ID where subjects touch.

| Document | Prefix | Covers |
|---|---|---|
| [requirements/naming.md](requirements/naming.md) | `FR-NAME` | Name syntax, owner namespace, case handling, reserved names, `.git` in URLs, redirects after renames and transfers |
| [requirements/repositories.md](requirements/repositories.md) | `FR-REPO` | Create, settings, visibility setting, rename, transfer, archive, delete, fork, mirror, import, adopt, list |
| [requirements/git-protocol.md](requirements/git-protocol.md) | `FR-GIT` | Smart HTTP and SSH transports, protocol v2, shallow and partial clone, archives, push hook points, LFS |
| [requirements/auth.md](requirements/auth.md) | `FR-AUTH` | Accounts, registration, passwords, magic links, passkeys, tokens, SSH and deploy keys, SSO (OIDC, SAML via a bridge, SCIM), LDAP, 2FA, sessions, sudo mode, device flow, Klotho as an OAuth2 provider |
| [requirements/access-control.md](requirements/access-control.md) | `FR-ACL` | Permission levels, orgs and teams, visibility enforcement, token scopes, protected branches and tags |
| [requirements/storage.md](requirements/storage.md) | `FR-STOR`, `NFR-STOR` | Metadata store, ID-based layout, atomic create, disk/metadata consistency, durability, integrity checks, housekeeping, backup and restore, quotas |
| [requirements/api.md](requirements/api.md) | `FR-API` | Versioning and compatibility, resource shapes, timestamps, pagination, errors, caching headers |
| [requirements/web-ui.md](requirements/web-ui.md) | `FR-UI`, `NFR-UI` | Browsing code and history, rendering, search, settings pages, the Moirai thread view and Mermaid diagrams, accessibility, i18n |
| [requirements/collaboration.md](requirements/collaboration.md) | `FR-COLLAB` | Pull requests, review, merge strategies, issues, releases, notifications |
| [requirements/integrations.md](requirements/integrations.md) | `FR-INT` | Webhooks, commit statuses, the Lachesis and Atropos hand-off, package registries, federation |
| [requirements/security.md](requirements/security.md) | `NFR-SEC` | TLS and proxies, safe user content, CSP and CSRF, safe diagram rendering, abuse limits, error disclosure, SSRF, subprocess safety, secrets at rest, audit log |
| [requirements/performance.md](requirements/performance.md) | `NFR-PERF` | Reference environment, git throughput, API and UI latency, streaming, memory, startup, scale, caching |
| [requirements/operations.md](requirements/operations.md) | `NFR-OPS` | Packaging, platforms, git dependency, configuration, logging, metrics, health checks, shutdown, migrations, admin CLI |

## Conventions

**IDs.** `FR-<AREA>-NNN` marks a functional requirement and `NFR-<AREA>-NNN` a non-functional one. IDs are grouped in tens by section and are never reused or renumbered: a dropped requirement is marked *withdrawn*, not deleted.

**Keywords.** Used as in RFC 2119:

- **must**: required for the requirement to be met.
- **should**: expected unless there is a documented reason not to.
- **may**: optional.

**Priority** says when a requirement is needed. It is separate from the keyword, which says how strict the requirement is once it's built.

- **must-have**: needed before Klotho can be recommended for any real use.
- **should-have**: expected in a 1.0 release.
- **nice-to-have**: welcome, not planned.

**Conflicts.** Each requirements file ends with a *Conflicts with the current implementation* table, checked against a named commit:

- **Conflict**: the current code behaves differently from the requirement.
- **Partial**: the requirement is partly met.
- **Missing** / **Not ensured**: nothing contradicts the requirement, but a building block it depends on is absent.

Requirements that simply haven't been implemented yet are not listed as conflicts.

## Most important conflicts today

Checked against commit `6b4895f`.

1. **Anyone can push.** There is no authentication or authorisation on git or the API ([FR-ACL-001](requirements/access-control.md), [FR-AUTH-020](requirements/auth.md)).
2. **Casing is discarded.** `MyRepo` is stored and shown as `myrepo` ([FR-NAME-020](requirements/naming.md)). Fixing it needs a metadata store ([FR-STOR-001](requirements/storage.md)).
3. **Repositories named `con`, `nul`, `aux`, … can be created on Windows but can't be cloned** (verified: HTTP 500) ([FR-STOR-007](requirements/storage.md)).
4. **Repository creation isn't atomic** ([FR-STOR-005](requirements/storage.md)), and **pushes have no size limit or subprocess cap** ([NFR-SEC-020](requirements/security.md), [NFR-SEC-022](requirements/security.md)).
5. **No graceful shutdown**, so restarting cuts off in-flight pushes ([NFR-OPS-030](requirements/operations.md)).
