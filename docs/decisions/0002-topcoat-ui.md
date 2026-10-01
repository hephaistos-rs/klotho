# ADR 0002: Server-rendered web UI with Topcoat

- **Status:** accepted
- **Date:** 2026-10-01
- **Supersedes:** the *Frontend* section of [ADR 0001](0001-stack.md). Everything else in ADR 0001 (gix + the `git` program, axum, sqlx, figment, russh, utoipa) still stands.

## Context

ADR 0001 chose a SvelteKit single-page app embedded in the binary. Its strongest rival was server-rendered HTML (askama + htmx). That option lost for one reason only: it meant two ways of building UI, templates *and* components, for one developer to maintain.

[Topcoat](https://github.com/tokio-rs/topcoat) (tokio-rs, first released 2026-07-22, now 0.9.0) removes that objection. It is a full-stack Rust framework:

- **Pages and components are async Rust functions** (`#[page]`, `#[component]`, `view!`), rendered on the server. They can call the database and check permissions directly.
- **Interactivity without wasm or Node.** Expressions wrapped in `$(...)` are type-checked Rust that Topcoat also translates to JavaScript. `#[shard]` components re-render on the server when their inputs change and get swapped into the page.
- **Batteries we would otherwise write:** cookies (signed, `__Host-` prefixed), session tokens with hashed storage that the app owns, an origin check (`Sec-Fetch-Site`), content-hashed assets, Tailwind without Node, mail, and vendored UI components (Topcoat UI, in the style of shadcn/ui).
- **It sits inside axum.** `topcoat::router::tower::TowerService::new(router)` is a tower service, so axum can mount it as its fallback.

So we get one UI technology (Rust components), server rendering, and no Node toolchain, all at once.

## Decision

### The UI is a Topcoat app in its own crate, mounted inside axum

- A new crate, **`klotho-web`**, holds every page, component and asset. It depends on `klotho-core` and nothing HTTP-specific from `klotho-server`.
- **axum stays the outer server.** It routes, in order: git transport, `/api/v1`, `/-/health` and other operations endpoints, `/-/assets/*`, and then `.fallback_service(TowerService::new(klotho_web::router(state)))`. Git transport and the JSON API never pass through Topcoat. They need streaming bodies, `401` challenges and OpenAPI, and Topcoat's own docs point at axum for those.
- Shared state (database pool, config, `RepoStore`) goes to Topcoat as *app context* (`.app_context(value)`).
- **Pin the exact version** (`topcoat = "=0.9.0"`) and upgrade on purpose, one version at a time.

### The UI and the API share services, not HTTP

- Pages call `klotho-core` service functions directly. They never call `/api/v1` over HTTP.
- **API parity (FR-API-004) is a working rule, not something the architecture enforces any more.** A page may only get its data and make changes through a public `klotho-core` service function. Any PR that adds a UI action adds the matching API endpoint in the same PR. The one exception is account security (credentials, second factors, sessions), which is UI-only. See the FR-API-004 rewording below.
- **The JSON API accepts bearer tokens only** (PATs, and OAuth2 access tokens from Phase 8). The session cookie means nothing to `/api/v1`. That removes CSRF from the API completely and leaves the sign-in endpoints in one place. `utoipa` still generates `/api/v1/openapi.json`, now for CLIs and integrations rather than for our own frontend.

### Sessions and CSRF

- Sessions use `topcoat::session`. It mints a 32-byte token, sets the `__Host-` cookie and hands us the SHA-256 `TokenHash`. The `sessions` table in `klotho-core` stays as designed in [auth-flows.md](../design/auth-flows.md). We store the hash, the expiry and `sudo_until`, and we can list and revoke sessions.
- CSRF is handled by Topcoat's origin check (`Sec-Fetch-Site`, falling back to `Origin`) and by `SameSite=Lax`. Because the API is bearer-only, nothing else carries cookie authority.

### Interactivity: `$(...)` and shards first, plain JavaScript only where we must

- Use Topcoat's `$(...)` expressions and `#[shard]` components for interactive parts. Don't mix in the htmx, Alpine AJAX or Datastar integrations, so the UI keeps a single approach.
- Pages work without JavaScript. Links are real `<a>` elements and changes are real `<form method="post">`s. Interactivity improves pages but is never required to read them (NFR-UI-002 restored).
- **Third-party JavaScript has no npm.** We vendor release files and declare them with `asset!()`:
  - **Mermaid:** its prebuilt ESM bundle, loaded only on pages that draw a diagram.
  - **Passkeys:** about 50 lines of our own script around `navigator.credentials`, using `PublicKeyCredential.parseRequestOptionsFromJSON()` / `parseCreationOptionsFromJSON()` and `toJSON()`. This replaces `@simplewebauthn/browser`.
  - **CodeMirror** has no prebuilt bundle. The diff and code views are server-rendered (`syntect`), so we may never need it. If Phase 9 does, that phase decides between a vendored one-off bundle and an alternative.

### Styling

Tailwind through Topcoat's `tailwind` feature (no Node), plus Topcoat UI components copied in with `topcoat ui add` and edited freely. They are our code once copied, so this isn't a dependency on a component library.

### Assets inside the single binary

Topcoat writes its asset bundle to a directory *after* building the program: `topcoat asset bundle` scans the built binary for `asset!()` declarations. That includes Topcoat's own runtime script and the generated Tailwind stylesheet. By default the binary then loads that directory from next to itself, which breaks NFR-OPS-001 (a single binary).

An asset's ID is a hash of (crate name, source file, asset path, options) and does not depend on the binary. So we use a **two-pass release build**:

1. `cargo build --release`, then `topcoat asset bundle --release --out target/klotho-assets`.
2. `cargo build --release` with `KLOTHO_ASSETS_DIR=target/klotho-assets` (originally planned as `--features embed-assets`; see the update below). This embeds `target/klotho-assets` with `rust-embed` and `include_str!`s its `manifest.toml`.
3. At runtime, `AssetConfig::hosted_at("/-/assets", Manifest::parse(EMBEDDED_MANIFEST))` makes Topcoat render URLs under `/-/assets/…`, and an axum route serves the embedded files with `immutable` caching.
4. **Check:** copy the final binary alone into an empty folder, start it, and require the home page and the stylesheet it links to to be served. If a future Topcoat release makes IDs depend on the binary, the page fails to render and the build fails, instead of the release panicking at runtime.

A `cargo xtask dist` command wraps all four steps, and CI runs it. In development, `topcoat dev` with `AssetBundle::load()` works as normal (`cargo xtask dev`).

> **Updated 2026-10-01, after the Phase 0 spike.**
> - Step 4 was planned as "bundle again from the final binary and compare manifests". That can't be done: `topcoat asset bundle` always rebuilds before scanning and has no `--features` flag. Running the final binary checks the same thing more directly.
> - **Step 2 isn't a Cargo feature.** Asset IDs hash the asset's path, and the Tailwind stylesheet's path is in `klotho-web`'s `OUT_DIR`. Any Cargo feature can change `klotho-web`'s `OUT_DIR` through feature unification. It happened in Phase 1, when the embedding dependencies turned on extra `windows-sys` features. So step 2 is `cargo build --release` with `KLOTHO_ASSETS_DIR` set, and `klotho-server`'s `build.rs` turns that into a crate-local `cfg(embed_assets)`. The embedding dependencies are always compiled, so both passes resolve the same dependencies.

We'll also ask upstream for first-class embedded bundles. If that lands, the two-pass build goes away.

## Alternatives considered

| Option | Why not |
|---|---|
| **Keep SvelteKit SPA (ADR 0001)** | Still workable, and still the fallback if Topcoat fails us. But it means two languages, Node at build time, no browsing without JavaScript, link previews that need the server to inject tags into a shell, and a generated-types pipeline to keep in sync. Topcoat removes all of these. |
| **askama + htmx** | Server-rendered and stable, but templates and Rust logic live apart, and anything interactive means hand-written JavaScript. Topcoat gives the same rendering model with type-checked components. Topcoat's htmx integration means we could move part of the UI to this style if needed. |
| **Leptos / Dioxus** | Still wasm for interactivity, which ADR 0001 rejected for build time and bundle size. Topcoat needs no wasm. |
| **Topcoat for everything, axum dropped** | Topcoat has no OpenAPI generation yet (on its roadmap), and git transport needs low-level control of bodies, status codes and challenges. axum already does all of that and stays. |

## Consequences

**Good:**
- One language for the whole codebase. Shared types are just Rust types.
- No Node.js at build time or at run time (NFR-OPS-005 strengthened).
- Browsing works without JavaScript (NFR-UI-002 restored). First visits are fast, link previews and status codes work with nothing extra, and search engines can index public pages.
- Pages run `authorize()` on the server, so there's no client-side permission logic to get wrong.
- Less code to write: no HTML shell server, no `/resolve` round trip for page URLs, no generated TypeScript client, no meta-tag injection.

**Bad:**
- **Topcoat is experimental** ("expect breaking changes"), three months old, and has a single maintainer team. Limits on the damage: it lives only in `klotho-web`, the version is pinned, and the API and `klotho-core` don't know it exists. If we have to leave it, we rewrite pages, not the platform.
- **Missing pieces we write ourselves for now:** form parsing and validation helpers (on Topcoat's roadmap), and rate limiting (stays in axum with `tower_governor`).
- **API parity is no longer automatic.** Holding to it now depends on the working rule above.
- **A two-pass release build** until upstream supports embedded bundles.
- **Fewer people know Topcoat** than Svelte, for future contributors.

**Requirement changes made with this decision:**
- **Restored:** NFR-UI-002 (read-only browsing without JavaScript).
- **Withdrawn:** NFR-UI-006 (the "JavaScript required" message; no longer needed).
- **Reworded:** FR-UI-041, FR-UI-043, FR-UI-050, FR-UI-051, FR-UI-052, NFR-PERF-012, NFR-PERF-015, NFR-OPS-005, FR-API-004, FR-NAME-032 (reserves `_topcoat` instead of `_app`).

**Revisit if:**
- A Topcoat release breaks us badly two upgrades in a row, or the project stalls (no release in six months).
- We need offline or heavily client-side features (for example a full in-browser editor) that `$(...)` and shards can't express.
