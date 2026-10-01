# ADR 0001: Technology stack

- **Status:** accepted; the *Frontend* section is superseded by [ADR 0002](0002-topcoat-ui.md) and the *Git* section by [ADR 0004](0004-native-git-transport.md) (both 2026-10-01)
- **Date:** 2026-09-30

## Context

Klotho is a self-hosted git platform written in Rust, mostly by one developer. The requirements that shape the stack are:

- **Single binary** whose only runtime dependency is `git` (NFR-OPS-001).
- **SQLite by default, PostgreSQL optionally** (FR-STOR-002).
- **Git over HTTP and SSH** (FR-GIT-001, FR-GIT-002).
- **An API that covers everything the UI does** (FR-API-004).
- **Small memory footprint** (NFR-PERF-020).

We also want to avoid adopting frameworks for their own sake.

## Decision

### Git: gix for reading, the `git` program for the wire protocol

> **Superseded by [ADR 0004](0004-native-git-transport.md).** Klotho does all git work in-process with gitoxide and never runs the `git` program. This section is kept for the record.

- **gix** reads repositories: refs, commits, trees, blobs and diffs.
- **`git upload-pack` / `git receive-pack`**, run as subprocesses, serve clone, fetch and push over both HTTP and SSH.
- **Hooks for pushes** (FR-GIT-020, FR-GIT-021) go through the `receive-pack` hook mechanism.

gix does not implement the server side of the git protocol. Git's own implementation is the reference one and gets protocol updates first. Gitea, Forgejo and GitLab (Gitaly) all run the `git` program in the same way.

### Backend

| Layer | Choice |
|---|---|
| HTTP | `axum` + `tower-http` |
| Database | `sqlx` (SQLite + PostgreSQL, compile-time checked queries, built-in migrations) |
| Configuration | `figment` (TOML file + environment variables) |
| SSH | `russh` |
| OpenAPI | `utoipa`, generated from the handler code |

### Frontend: SvelteKit single-page app with the static adapter

> **Superseded by [ADR 0002](0002-topcoat-ui.md).** The UI is now server-rendered with Topcoat. This section and the frontend parts of *Consequences* are kept for the record.

- **SvelteKit** with `@sveltejs/adapter-static` in SPA mode (`fallback` page, `ssr = false`), written in TypeScript.
- The built files are embedded in the binary with `rust-embed`. Node.js is needed only at build time (NFR-OPS-005).
- The frontend talks only to the public `/api/v1` API, using TypeScript types generated from the OpenAPI document (`openapi-typescript` + `openapi-fetch`). The Rust structs are the single source of truth for types on both sides.
- Mermaid is loaded only on the pages that use it.

## Alternatives considered

| Option | Why not |
|---|---|
| **Server-rendered HTML (askama) + htmx, with Svelte islands later** | The strongest alternative. It is better for no-JS browsing and first load. It was rejected because it means two ways of building UI (templates *and* components) for one developer to maintain. Most Klotho traffic will be signed-in team members, which suits an SPA. |
| **SvelteKit with `adapter-node` (server rendering)** | Needs Node.js at runtime, which breaks the single binary. |
| **Leptos (Rust → wasm) in islands mode** | All-Rust and shares types directly. But it's pre-1.0 with API churn, adds slower builds for two targets, and wasm isn't faster for UI work. The OpenAPI → TypeScript pipeline gets most of the shared-types benefit. |
| **React** | Its best current features (Server Components, Server Functions) need a Node server. As a client-only framework its runtime is much larger than Svelte's, and its ecosystem advantage is small here: the heavy libraries Klotho needs (CodeMirror 6, Mermaid) are framework-agnostic. |
| **Preact** | Smaller than React, but lags on React 19 features. It doesn't beat Svelte on size or simplicity. |

## Consequences

**Good:**
- One UI technology to learn.
- Every UI feature is available through the API by construction.
- No runtime Node.
- Type-safe API calls, with types generated from the Rust code.

**Bad:**
- No browsing without JavaScript (NFR-UI-002 withdrawn, replaced by NFR-UI-006).
- First visits are slower than server-rendered pages (NFR-PERF-012 reworded, NFR-PERF-015 added).
- Link previews need the server to insert meta tags into the HTML shell (FR-UI-051).
- Contributors need Node.js to build from source.

**Requirement changes made with this decision:**
- **Withdrawn:** NFR-UI-002.
- **Added:** NFR-UI-006, FR-UI-050 to 052, NFR-PERF-015, NFR-OPS-005.
- **Reworded:** FR-UI-041, FR-UI-043, NFR-PERF-012.
- **Extended:** FR-NAME-032, which now reserves `_app`.

**Revisit if:** Klotho is increasingly used for large public instances where anonymous first-visit speed and search engine indexing matter. The API-only frontend makes it possible to add server-rendered public pages later without touching the rest.
