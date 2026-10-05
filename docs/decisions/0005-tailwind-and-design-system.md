# ADR 0005: Tailwind, Topcoat UI and the Klotho design system

- **Status:** accepted
- **Date:** 2026-10-03
- **Supersedes in part:** the *Styling* section of [ADR 0002](0002-topcoat-ui.md), which said only "Tailwind through Topcoat, plus Topcoat UI components". Everything else in ADR 0002 (Topcoat as the UI's router, sessions and cookies; axum outside it; the two-pass embedded build) still stands.

## Context

Before this decision the UI had six pages styled with raw Tailwind palette classes (`bg-gray-900`, `text-red-700`, …), no shared components beyond a few helpers, no dark mode, no focus styles of its own, system fonts, and a Tailwind CLI that `build.rs` downloaded without checking it. Phase 3 is about to add the browsing UI (file tree, code, history, diffs), which needs a real design system first: one set of tokens, components with every state, and a look that belongs to Klotho rather than to a component library's defaults.

The constraints come from earlier decisions and requirements:
- **Single binary, no Node** (NFR-OPS-001, NFR-OPS-005). Building must not need Node.js, and every web asset (stylesheets, fonts, icons) must be inside the release binary (FR-UI-052).
- **Works without JavaScript** (NFR-UI-002), **WCAG 2.2 AA** (NFR-UI-001), **360 px and up** (NFR-UI-003), **light and dark with an override** (NFR-UI-005), **at most 100 KB of CSS and JS per page** (NFR-PERF-015).
- **Reproducible and offline builds:** nothing fetched at build time may change without a commit, and a packager must be able to build with no network.

## Decision

### Tailwind v4 through Topcoat, themed by CSS variables

- `crates/web/styles.css` is the only stylesheet. It imports Tailwind with `source(none)` and scans `src/**/*.rs`, so only classes that appear literally in our Rust source end up in the CSS.
- **Tokens, not palette classes.** `:root` defines the light tokens (`--background`, `--primary`, `--destructive-soft`, `--shell`, `--thread`, …) in OKLCH; `@theme inline` maps each to a Tailwind colour, so markup says `bg-primary` or `text-muted-foreground`, never `bg-indigo-700`. Raw palette utilities and arbitrary colour values are not used in markup (rubric R1; a grep in review enforces it).
- **Dark mode:** the dark tokens appear twice, in `:root.dark` and in `@media (prefers-color-scheme: dark) { :root:not(.light) }`, and a block `@custom-variant dark` covers both. So `<html>` with no class follows the system, and `class="light"` / `class="dark"` override it, all without JavaScript.
- `--font-weight-semibold` is 650 (titles), `--text-meta` is 13 px for details, radius is 2/4/6 px, and shadows exist only for floating surfaces.
- Generated CSS is about 7.5 KB gzipped (NFR-PERF-015).

### Topcoat UI components, vendored and rethemed

`topcoat ui init --theme neutral` and `topcoat ui add` copied components into `crates/web/src/components/`; `components.toml` records them. Once copied they are our code, and they have been rethemed to the tokens: soft-fill destructive and success alerts that draw their own icon, a Warning badge that always carries an icon, an outlined destructive button for list actions, a mono Ref badge that truncates, flat cards, `border-input` controls with a 3:1 boundary, `--ring` focus with `outline-hidden` so forced-colors mode still shows focus, scroll containers that keyboard users can focus. Every interactive component works without JavaScript (menus are `<details>`, selects are native).

Only the components and variants a page uses are kept, because unused code fails the `-D warnings` build and we don't silence it with `allow`s. Today that is alert, badge, button, checkbox, dropdown menu, field, input, label and select. `topcoat ui add <name>` brings a component back when a page needs it (tables, tabs and breadcrumbs for the Phase 3 browser), and it is rethemed to the tokens and DESIGN.md before use. The Windows CLI writes `components.toml` paths with backslashes, so after every `add` or `remove` normalise them to `/` (in Git Bash: `tr '\134' / < components.toml > t && mv t components.toml`).

`crates/web/src/ui.rs` holds the shell (skip link, night header with the spindle mark and the thread edge, the mobile `<details>` menu, the footer theme switch) and a few shared page helpers (`page_header`, `section_heading`, `list`/`list_row`, `empty_state`, `notice`, `field`, `form_error`, `form_success`, `code_line`).

### Topcoat features

| Feature | State | Why |
|---|---|---|
| `default` (router, session, cookie, serve, compression, asset, font, icon, runtime, view, discover) | **on** | Topcoat is the UI's router, session and cookie layer (ADR 0002). The design-run brief proposed turning the defaults off and leaving routing and sessions to axum; that would have rewritten ADR 0002, so the defaults stay. |
| `tailwind` (dependency and build-dependency) | on | Tailwind without Node. |
| `tower` | on | axum mounts the Topcoat router as a tower service. |
| `ui` | **new** | The component registry behind `topcoat ui`. |
| `icon-iconify` (dependency and build-dependency) | **new** | Iconify icon sets, compiled in as inline SVG. |
| `font-fontsource` | **off** | See *Fonts* below. |
| `htmx`, `datastar`, `alpine-ajax`, `sse`, `multipart`, `mail`, `sitemap`, `fs` | off | Not needed; ADR 0002 keeps one interactivity approach (`$(...)` and shards). |

### The Tailwind CLI: pinned, verified, or supplied

- `crates/web/build.rs` pins Tailwind **4.3.2** and holds its SHA-256 per platform (Linux x86_64 and aarch64, macOS x86_64 and aarch64, Windows x86_64), copied from the release's `sha256sums.txt`. Topcoat downloads the CLI once, checks it and caches it under `target/`.
- **No unverified downloads.** On a platform with no checksum in the table the build fails, unless `TAILWIND_CLI` names a local executable.
- **Offline or packaged builds:** set `TAILWIND_CLI=/path/to/tailwindcss` (any 4.x standalone CLI). Nothing is downloaded.
- **To upgrade:** change `TAILWIND_VERSION` and every hash in `TAILWIND_SHA256` together, from the new release's `sha256sums.txt`, then delete `target/topcoat/cache/tailwind` so the new CLI is fetched and checked. (Topcoat does not re-check a cached CLI.)
- Because `build.rs` prints `rerun-if-*` lines, it lists everything that shapes the stylesheet: `src`, `styles.css`, `icons` and `TAILWIND_CLI`.

### Fonts: vendored variable files, embedded

- **Atkinson Hyperlegible Next** (text) and **Atkinson Hyperlegible Mono** (code, refs, hashes, tokens), SIL OFL 1.1. They keep easily confused characters (`l 1 I`, `0 O`) apart, which is the job a forge's type has to do.
- **Cormorant Garamond** (page titles and the wordmark only), SIL OFL 1.1: the display serif of the Nyx direction (DESIGN.md P10).
- The files are Fontsource's **variable** WOFF2 builds, version **5.3.0**, Latin and Latin Extended subsets: six files, 152 KB in all, in `crates/web/assets/fonts/` with the three OFL licence texts beside them (`LICENSE-OFL-next.txt`, `-mono.txt`, `-cormorant.txt`).
- `crates/web/src/theme.rs` declares them with `font!` and `asset!`, split by `unicode-range`, weight range 200–800 (Cormorant 300–700), `font-display: swap`. Only the Latin text face is preloaded. The router serves the `@font-face` CSS at `/_topcoat/fonts/…` and the files come from the asset bundle, so release builds embed them like any other asset.
- **Why not `font-fontsource`:** its `fontsource_font!` macro fetches one static file per weight from `@latest` at build time. That can't give the 650 title weight (it needs the variable file), and `@latest` isn't reproducible.
- **To update the fonts:** download the new version's six files and licences, replacing `5.3.0` with the new version:
  ```sh
  cd crates/web/assets/fonts
  for f in next mono; do
    for s in latin latin-ext; do
      curl -fsSLo atkinson-hyperlegible-$f-$s-wght-normal.woff2 \
        "https://cdn.jsdelivr.net/fontsource/fonts/atkinson-hyperlegible-$f:vf@5.3.0/$s-wght-normal.woff2"
    done
    curl -fsSLo LICENSE-OFL-$f.txt \
      "https://cdn.jsdelivr.net/npm/@fontsource-variable/atkinson-hyperlegible-$f@5.3.0/LICENSE"
  done
  for s in latin latin-ext; do
    curl -fsSLo cormorant-garamond-$s-wght-normal.woff2 \
      "https://cdn.jsdelivr.net/fontsource/fonts/cormorant-garamond:vf@5.3.0/$s-wght-normal.woff2"
  done
  curl -fsSLo LICENSE-OFL-cormorant.txt \
    "https://cdn.jsdelivr.net/npm/@fontsource-variable/cormorant-garamond@5.3.0/LICENSE"
  ```
  Then compare the `unicode-range` lists in the package's `index.css` (same CDN, `@fontsource-variable/…@<version>/index.css`) with the `LATIN` and `LATIN_EXT` constants in `theme.rs`, and commit the files.

### Icons: one vendored Iconify set

- **Lucide** is the only icon set (ISC; a few icons derived from Feather are MIT, and both licences are in `crates/web/icons/LICENSE-ISC-lucide.txt`).
- `build.rs` stages it with `iconify::BuildConfig::new().cache_dir("icons").icon_set_version("lucide", LUCIDE_VERSION)`. The set (`@iconify-json/lucide` **1.2.138**) is committed as `crates/web/icons/lucide.json`, with `lucide.version` beside it, so builds need no network.
- Pages use `iconify_icon!("lucide:<name>")`. Icons compile into inline SVG: no asset and no request. An icon without a label is `aria-hidden`.
- **To update the icons:** change `LUCIDE_VERSION` in `build.rs` and build once with network access; Topcoat downloads that version into `crates/web/icons/` and rewrites `lucide.version`. Refresh the licence from `https://raw.githubusercontent.com/lucide-icons/lucide/main/LICENSE`, check that every `iconify_icon!` name still exists (the build fails if one doesn't), and commit the three files.

### Every asset is in the binary, and the release build proves it

The two-pass build of ADR 0002 already embeds anything declared with `asset!()`, so the stylesheet and the fonts needed no change in `crates/server/src/assets.rs`. `cargo xtask dist` now checks more: it starts the lone binary, fetches the home page, and requires every stylesheet and preload it links, and every `url()` inside those stylesheets, to return 200 with `immutable` caching. The page may not reference another host. The last run served 4 stylesheets and the 7 files they load.

### Theme preference: a UI-only exception to API parity

The theme switch (NFR-UI-005) is a footer form posting to `POST /-/theme`. It sets the `klotho_theme` cookie (`HttpOnly`, `SameSite=Lax`, `Secure` over HTTPS, one year; "System" clears it) and redirects back to a same-site path. The server renders the class on `<html>`, so there is no flash and no JavaScript.

It has **no API endpoint and no `klotho-core` service**: it is a per-browser display preference, stored nowhere but the cookie. This is a second exception to the "UI and API move together" rule (FR-API-004), next to account security. If the preference ever moves to the account (synced across devices), it becomes a core service with an API endpoint like any other setting.

### The design direction

The full system is in the root [`DESIGN.md`](../../DESIGN.md); the product context is in [`PRODUCT.md`](../../PRODUCT.md). In short: the name comes from Klotho, who spins the thread of life, and a forge's content is threads. The direction is **Nyx**: the Fates are daughters of Night. The header is night in both themes; at night the page is night with linen text and **spun gold** for action, links and focus; by lamplight (the light theme) it is linen and ink with **bronze** for action. **Madder** is danger, **olive** success, **amber** warnings, and the gold is also the thread. Page titles and the wordmark are set in Cormorant Garamond; everything else is Atkinson Hyperlegible. The one signature is **the thread**: a 2 px gold line along the bottom of the night header, off the top of the spindle mark, under the current navigation item, and (from the commit log on) down the history. It is decorative and `aria-hidden`; it never carries meaning on its own. Everything else is quiet: flat surfaces, small radii, plain copy.

Proposals made during the design run and what was decided:

| # | Proposal | Decision |
|---|---|---|
| P1 | Natural-dye palette (indigo, madder, verdigris, weld) | Accepted, then replaced the same day by P10 |
| P2 | The thread as the signature | Accepted for the header edge, the current-nav marker and the commit log only; thread and beads are `aria-hidden` |
| P3 | Warning colour | Accepted, moved to amber (hue 55–60) so it can't be mistaken for the thread; always an icon plus text |
| P4 | Atkinson Hyperlegible Next and Mono, self-hosted | Accepted, with semibold at 650 |
| P5 | Mobile header folds links into a `<details>` menu | Accepted |
| P6 | Frame widens to `max-w-6xl`; forms stay `max-w-md` | Accepted; long code lines scroll inside their block, never the page |
| P7 | Theme toggle as a cookie set by `POST /-/theme` | Accepted, as the UI-only exception above |
| P8 | A drawn spindle mark beside the wordmark | Rejected at first; added with P10 as an inline `aria-hidden` SVG |
| P9 | Lucide as the one icon set | Accepted |
| P10 | Nyx: night header, gold at night, bronze by lamplight, Cormorant Garamond titles | Accepted, chosen from three directions (Nyx, Tyrian, Black-figure) on a design canvas; a purple light theme was rejected |

A rubric (`.impeccable/klotho-rubric.md`) turns the direction into checks: tokens only, both themes complete, works without JS, AA contrast, visible focus, self-contained, the thread only where allowed, warnings with icons, long lines scrolling in their block. Design changes are graded against it.

## Alternatives considered

| Option | Why not |
|---|---|
| **Keep raw Tailwind palette classes** | No dark mode without doubling every class, no single place to change a colour, and contrast checked page by page. |
| **Topcoat UI's Neutral theme as shipped** | The generic component-library look; borders below 3:1 on inputs, shadows on cards, no success or warning states. We keep its structure and replace its look. |
| **Hand-written CSS without Tailwind** | Topcoat UI components are written in Tailwind classes, and Tailwind needs no Node here. A second styling approach would split the codebase. |
| **`font-fontsource`** | Static per-weight files from `@latest`: no 650 weight, not reproducible. |
| **System fonts only** | Smaller, but loses the character distinctions refs, hashes and tokens need, and the identity. 152 KB, mostly cached forever, is acceptable. |
| **Turning Topcoat's default features off** | Would move routing, sessions and cookies to axum and undo ADR 0002. |

## Consequences

**Good:**
- One token set drives light and dark; contrast is measured once, in DESIGN.md, for every pair the components use.
- Every web asset is in the binary and `cargo xtask dist` proves it on each release build.
- Builds are reproducible and can run offline: Tailwind by checksum or `TAILWIND_CLI`, fonts and icons from the repository.
- Pages are assembled from rethemed components and helpers, so new pages start accessible and on-brand.

**Bad:**
- 152 KB of fonts and a 600 KB icon JSON in the repository (only the icons used are compiled in).
- Vendored components drift from upstream Topcoat UI; picking up upstream changes means comparing with a fresh `topcoat ui add` in a scratch package and merging by hand.
- Topcoat does not re-verify a cached Tailwind CLI; the cache in `target/` is trusted after the first check.
- A second UI-only endpoint (`/-/theme`) outside API parity.

**Requirements:** NFR-OPS-005, FR-UI-052, NFR-PERF-015 (assets embedded, no Node, CSS budget); NFR-UI-001, NFR-UI-002, NFR-UI-003 (accessibility, no JavaScript, narrow screens); NFR-UI-005 (light and dark with an override); FR-API-004 (the theme exception).
