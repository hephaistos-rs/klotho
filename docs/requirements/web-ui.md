# Web UI requirements

**Scope:** the browser interface for reading code and history and for managing settings. That covers browsing, rendering, search, and the UI's own accessibility and localisation.

**How it's built:** server-rendered with Topcoat, mounted inside axum, with assets embedded in the binary ([ADR 0002](../decisions/0002-topcoat-ui.md)).

**Not in scope:** issue, PR and review screens ([collaboration.md](collaboration.md)), the data behind the pages ([api.md](api.md)), browser security headers ([security.md](security.md)), and page latency ([performance.md](performance.md)).

## Browsing

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-UI-001 | Users **must** be able to browse a repository's file tree and view files at any branch, tag or commit. | Core feature of all four. | must-have |
| FR-UI-002 | The repository home page **must** show the README of the default branch, rendered from Markdown (CommonMark + GitHub Flavored Markdown tables, task lists and autolinks). | All four render README files. GFM is the de-facto standard. | must-have |
| FR-UI-003 | Source files **must** be shown with syntax highlighting and line numbers. Binary files **must** show a download link instead of their content. | All four. | must-have |
| FR-UI-004 | Images (PNG, JPEG, GIF, WebP) **should** be previewed inline. SVG **should** be previewed only as an `<img>` or from a sandboxed origin, never inline in the page. | All four preview images. Inline SVG can run scripts. | should-have |
| FR-UI-005 | Users **must** be able to view the commit log for a branch or path, and any single commit with its diff. | All four. | must-have |
| FR-UI-006 | Users **should** be able to view blame for any file. | All four. | should-have |
| FR-UI-007 | Users **must** be able to list branches and tags, with each one's latest commit and date. | All four. | must-have |
| FR-UI-008 | Users **should** be able to compare two refs and see the diff and the commits between them. | All four (`/compare/a...b`). | should-have |
| FR-UI-009 | Users **should** be able to link to a line or line range and turn any link into a permalink pinned to a commit ID. | GitHub (`#L10-L20`, `y` key). Shared links stay correct after the branch moves on. | should-have |
| FR-UI-010 | The repository page **must** show HTTPS and SSH clone URLs with a copy button. | All four. | must-have |
| FR-UI-011 | Commits **may** show whether their GPG or SSH signature is verified. | All four show signature status badges. | nice-to-have |

## Navigation and search

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-UI-020 | Users **must** be able to search repositories they can see by name, description and topic. | All four. | must-have |
| FR-UI-021 | Users **should** be able to search code within a repository. Instance-wide code search **may** be added later. | Gitea (with bleve/Elasticsearch), GitLab and GitHub. | should-have |
| FR-UI-022 | Users **should** be able to jump to a file by typing part of its path. | GitHub `t`, GitLab and Gitea "Go to file". | should-have |
| FR-UI-023 | Users **must** have a dashboard of their own repositories and recent activity, and owners **must** have a profile page listing their public repositories. | All four. | must-have |

## Settings

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-UI-030 | Every repository, user, organisation and instance setting required elsewhere in these documents **must** be reachable from the UI, with the setting pages served under `/-/` as FR-NAME-034 requires. | Admins shouldn't need the API or CLI for routine tasks. | must-have |

## Thread view and diagrams

The **thread view** follows one commit through all three Moirai: *spun* by Klotho (pushed), *measured* by Lachesis (CI), and *cut* by Atropos (released or deployed). Klotho draws it, and the commit graph, with [Mermaid](https://mermaid.js.org/). The server generates the diagram source as text, and Mermaid renders it in the browser, so Klotho needs no drawing code of its own.

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-UI-040 | Each commit page **should** show the commit's thread as three stages in order. **Spun:** who pushed it and when. **Measured:** its commit statuses (FR-INT-010). **Cut:** the tags and releases containing it and its deployments (FR-INT-013). Each stage **must** be marked done, pending, failed or not started. | Klotho's own idea, making the README's "first commit to running application" journey visible. Git doesn't record who pushed a commit, so this needs Klotho to keep the push events from FR-GIT-021. | should-have |
| FR-UI-041 | The thread view and the commit graph **should** be rendered by Mermaid: a `flowchart` for the thread and a `gitGraph` for the commit graph. The server writes the diagram source into the page as text, and Mermaid draws it in the browser. The API still returns data only (`/graph`, `/thread`). | GitHub, GitLab and Gitea already ship Mermaid for Markdown. It saves writing custom SVG layout code. | should-have |
| FR-UI-042 | The commit graph **should** show at most 50 commits at a time, with paging to older commits. | Mermaid's `gitGraph` gets slow and hard to read beyond a few dozen commits. It also can't draw octopus merges (three or more parents), so the server has to simplify those. | should-have |
| FR-UI-043 | Mermaid **should** be loaded only on pages that contain a diagram, as a module script that the page includes only when it draws one. Without JavaScript, the diagram source **should** stay readable as text. The same information **must** also appear as text, for screen readers (NFR-UI-001). | The Mermaid bundle is large. A diagram alone is invisible to screen readers. | should-have |
| FR-UI-044 | Fenced ```` ```mermaid ```` code blocks in rendered Markdown (READMEs, issues, pull requests) **should** be displayed as diagrams. | GitHub (since 2022), GitLab and Gitea all do this. Once Mermaid is in the bundle for FR-UI-041, it costs almost nothing. | nice-to-have |

## Delivery

The UI is a Topcoat app (crate `klotho-web`) that axum mounts as its fallback service. Pages are rendered on the server. See [ADR 0002](../decisions/0002-topcoat-ui.md).

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-UI-050 | Requests **must** be routed in this order: git transport, `/api/`, the `/-/` server endpoints, assets, and only then UI pages. Unknown paths under `/api/` **must** get a JSON 404. Any other unknown path **must** get an HTML 404 page. | A UI fallback that swallows API and git paths makes client errors confusing. | must-have |
| FR-UI-051 | Owner and repository pages **must** contain their own `<title>`, `description` and Open Graph tags. They **must** be served with status 404 when the owner or repository doesn't exist or isn't visible to the requester, with the same page in both cases (FR-ACL-013). | Link previews and crawlers depend on these tags and status codes. Server rendering makes this straightforward, but it still has to be done on purpose. | should-have |
| FR-UI-052 | Assets **must** have content-hashed filenames, be served under `/-/assets/` with `Cache-Control: public, max-age=31536000, immutable`, and be embedded in the binary (NFR-OPS-005). HTML pages **must** be served with `Cache-Control: no-cache`, plus `private` when the request is signed in. | Repeat visits load almost no assets, a new release takes effect immediately, and shared caches never store a signed-in user's page. | must-have |

## Quality

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| NFR-UI-001 | The UI **should** meet WCAG 2.2 level AA. | GitHub and GitLab both publish accessibility conformance targets. | should-have |
| NFR-UI-002 | Read-only browsing (repositories, files, history, commits, profiles) **should** work with JavaScript disabled, and so should password sign-in. Interactive features (live updates, diagrams, passkeys, copy buttons) **may** need JavaScript. | Withdrawn on 2026-09-30 with the SPA ([ADR 0001](../decisions/0001-stack.md)), and restored on 2026-10-01 because server rendering makes it nearly free ([ADR 0002](../decisions/0002-topcoat-ui.md)). It helps text browsers, locked-down environments and crawlers. | should-have |
| NFR-UI-006 | *Withdrawn 2026-10-01.* Was: a "JavaScript is required" message for no-JS visitors. NFR-UI-002 covers this case again. | — | — |
| NFR-UI-003 | The UI **should** be usable on screens 360 px wide and up. | All four have responsive layouts. People review code on phones. | should-have |
| NFR-UI-004 | Every UI string **should** be translatable, and the UI **may** ship with community translations. | Gitea/Forgejo (Crowdin/Weblate) and GitLab. | nice-to-have |
| NFR-UI-005 | The UI **may** follow the system's light or dark colour scheme and offer a manual override. | All four. | nice-to-have |

## Conflicts with the current implementation

Checked 2026-10-02, during Phase 3: no conflicts. The browse pages don't exist yet. The API's `raw` endpoint serves blobs as `application/octet-stream`, which already fits FR-UI-004's intent.
