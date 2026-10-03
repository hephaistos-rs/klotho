# Product

<!-- impeccable:product-schema 1 -->

> Written 2026-10-03 from the repository (README, ROADMAP, ADRs 0001–0004, docs/requirements) without an interview: the maintainer asked for no questions until review. Lines marked *(inferred)* are hypotheses for the maintainer to confirm or correct.

## Platform

web

## Users

- **The person who runs the instance.** A developer or small-team admin who self-hosts their own forge. They install one binary, edit one TOML file, point it at a data directory, and expect it to keep running unattended. They use the admin CLI (`klotho admin …`) when the web UI isn't up yet.
- **The people who use it every day.** Mostly signed-in members of the team that owns the instance (ADR 0001: "Most Klotho traffic will be signed-in team members"). Their jobs are reading code and history, managing their own credentials (tokens, later SSH keys and passkeys), and later reviewing pull requests. They review code on phones as well as desktops (NFR-UI-003).
- **Anonymous visitors** to public repositories, crawlers and link previews (FR-UI-051, NFR-UI-002). A secondary audience. *(inferred: small compared to team members on a typical instance)*

## Product Purpose

Klotho is a self-hosted git forge in Rust. It hosts repositories over HTTP and SSH, with accounts, access control and a web UI for browsing code and history, and later organisations, protection rules, integrations and collaboration (ROADMAP phases 3–9).

It is the first of the three Moirai in Hephaistos. Klotho *spins* the thread (code is born and pushed here), Lachesis *measures* it (CI), and Atropos *cuts* it (release and deploy). Success is a team running its whole code life on its own infrastructure, from first commit to running application, with one binary per stage.

## Positioning

- **One binary with no runtime dependencies.** Not even `git`: all git work, including the server side of the wire protocol, is Klotho's own code on gitoxide (ADR 0004). Gitea, Forgejo and GitLab all run the `git` program.
- **Server-rendered, works without JavaScript.** Read-only browsing and password sign-in work with JS off (NFR-UI-002). Pages stay light: at most 100 KB of gzipped CSS and JS on pages without diagrams (NFR-PERF-015).
- **The thread.** The commit page will show the commit's thread through all three Moirai, spun, measured and cut, each stage done, pending, failed or not started (FR-UI-040). No neighbouring forge can claim this, because it needs Lachesis and Atropos.

What Klotho deliberately isn't:
- Not a multi-tenant SaaS with GitHub's scale and social features. Comparisons in the docs are against Gitea, Forgejo, GitLab and GitHub, and favour what suits single-team installs. *(inferred from forge-comparison.md: "Low priority for single-team installs")*
- Not a single-page app, and not an in-browser IDE (ADR 0002 "Revisit if").
- Not a CI runner or a host. Those are Lachesis and Atropos.

## Operating Context

- Installs: one binary plus a config file (`klotho.example.toml`) and a data directory (ADR 0003). Built on Windows, run in production on Linux (NFR-OPS-002).
- Git clients talk to it over smart HTTP (Basic auth with a token as the password) and SSH. The JSON API (`/api/v1`) takes bearer tokens only.
- The web UI is mounted inside axum as a fallback. Every non-owner route lives under `/-/` (FR-NAME-034).
- Today's pages: home, sign in, register, sign out and personal access tokens. Phase 3 adds dashboard, explore, profile, repository home, tree, file, commits and commit.

## Capabilities and Constraints

- The UI is server-rendered Topcoat (`=0.9.0`), Tailwind with no Node, and vendored Topcoat UI components (ADR 0002).
- Every page works without JavaScript: links are `<a>` and every change is a `<form method="post">`. Runtime JS only enhances.
- Every asset (stylesheet, scripts, fonts, icons) is embedded in the release binary, with content-hashed URLs under `/-/assets/` and immutable caching (FR-UI-052, NFR-OPS-005). No third-party requests at runtime.
- "Not visible" must look exactly like "doesn't exist" (FR-ACL-013), including in the UI's 404s.
- Every UI action has an API endpoint that calls the same core service (FR-API-004).
- Mermaid, when it arrives (Phase 8), loads only on pages that draw a diagram.
- Terms: *repository*, *owner*, *ref*, *branch*, *tag*, *commit*, *access token*, *scope*. Non-git stored files are called *files*, never objects or blobs (ADR 0003).
- Undecided: translations (NFR-UI-004) and a CSP (NFR-SEC-012, should-have; forbids inline scripts).

## Brand Commitments

- The name is **Klotho**: the Fate who spins the thread of life. The sisters are Lachesis (measures) and Atropos (cuts). The README's line: "every repository starts here, and every commit adds to the thread."
- The parent project is **Hephaistos**, "forged in Rust".
- The thread vocabulary (spun, measured, cut) is product language (FR-UI-040), not decoration.
- Voice in the docs and UI copy: plain, short, exact, no hype. UI strings state what happens ("Copy your new token now. It won't be shown again.").
- There is no logo or visual identity yet. *(inferred from the repository: no image assets exist)*

## Evidence on Hand

- No screenshots, logos, testimonials, customers or benchmarks exist. Don't invent any.
- Real content to design with: the existing page copy in `crates/web/src/`, the scope names and descriptions in `klotho-core`, and the test repositories in `testRepos/`.

## Product Principles

1. **The tool disappears into the work.** People come to read code and manage access. Pages are quiet and dense where they need to be, and they look like a forge.
2. **Works without the extras.** No JavaScript, no external requests, no second file next to the binary. Every enhancement degrades to a working page.
3. **The thread is the one idea.** History as a spun thread (commits, branches, refs, and later spun → measured → cut) is where Klotho's identity lives. It shows up as meaning, never as costume.
4. **Exact and honest.** Identifiers in monospace, states named plainly, errors that say what went wrong and what to do. Nothing claims more than the server knows.
5. **Small and fast by default.** A strict CSS/JS budget and server rendering, so a phone on a slow link gets the page first.

## Accessibility & Inclusion

- WCAG 2.2 AA (NFR-UI-001), in both light and dark.
- Usable from 360 px wide (NFR-UI-003).
- Follows the system colour scheme, with a manual override (NFR-UI-005).
- Diagrams always have the same information as text (FR-UI-043).
- Strings are written to be translatable later (NFR-UI-004).
