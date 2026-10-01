# Performance requirements

**Scope:** speed, throughput, resource use and scalability.

**Not in scope:** abuse-driven caps such as rate limits and subprocess limits ([security.md](security.md)), and repository housekeeping ([storage.md](storage.md)).

**Reference environment.** Unless stated otherwise, every target below is measured on: 4 vCPU, 8 GiB RAM, local NVMe SSD, Linux x86_64, with the client on the same 1 Gbit/s LAN. The **reference dataset** is 10,000 repositories and 1,000 users, including one "large" repository with 1,000,000 commits, 50,000 refs and a 2 GiB pack. The Linux kernel is a suitable stand-in.

## Git operations

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| NFR-PERF-001 | A full clone over HTTP **should** take no more than 110% of the time the same clone takes from `git http-backend` behind a minimal web server on the same host. | Klotho delegates to `git upload-pack`, so its own overhead (proxying, auth, streaming) should be small. `git http-backend` is the natural baseline. | should-have |
| NFR-PERF-002 | A no-op fetch (client already up to date) of the large repository **should** finish in under 1 s at p95 with protocol v2. | Protocol v2 ref filtering makes this possible. CI polls often, so no-op fetches are the most common request. | should-have |
| NFR-PERF-003 | The server **must** handle 100 simultaneous clones of a medium repository (100 MiB) without any of them failing. Requests over the concurrency cap (NFR-SEC-022) wait in a queue instead of failing. | Many CI jobs start at once after a push. | must-have |
| NFR-PERF-004 | Pack data **must** be streamed between client and git in both directions. Server memory **must not** grow with pack size. | Gitea and GitLab stream packs. Buffering a 2 GiB push in memory would exhaust RAM. | must-have |

## API and UI latency

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| NFR-PERF-010 | API reads of a repository summary, a tree listing (≤ 1,000 entries), one commit, or one page of commits **should** have p95 latency under 200 ms on the reference dataset, at 50 concurrent requests. | These are the requests behind every repository page. 200 ms keeps pages feeling instant. | should-have |
| NFR-PERF-011 | Listing a page of repositories **should** have p95 latency under 100 ms regardless of how many repositories exist. | Listings should come from the metadata store, not a filesystem scan. | should-have |
| NFR-PERF-012 | On the reference dataset, the file tree, file and commit pages **should** have a server response time (time to first byte) under 300 ms at p95, and show their main content within 1 s of navigation start on a first visit. | Pages are rendered on the server ([ADR 0002](../decisions/0002-topcoat-ui.md)), so server time dominates what users see. Slow parts, such as each tree entry's last commit, **should** stream in later through a `suspense` boundary instead of holding up the page. They may also need caching (NFR-PERF-031). | should-have |
| NFR-PERF-013 | Downloading a file through the raw endpoint **must** stream it, so server memory doesn't grow with file size. | GitHub and Gitea stream raw blobs. Otherwise one request for a large file could exhaust memory. | must-have |
| NFR-PERF-014 | A commit log request for `N` commits **must** cost time roughly proportional to `N`, not to the length of the repository's history. | This holds for commits in date order. Filtering by path can't always meet it and should use the commit-graph Bloom filters (FR-STOR-031). | must-have |
| NFR-PERF-015 | The JavaScript and CSS loaded by any page without diagrams **should** total at most 100 KB gzipped. Mermaid and other large libraries **must** be loaded only on the pages that use them. | Server rendering means the browser downloads only CSS and Topcoat's small runtime, so a tight budget is realistic. Checking it in CI stops it growing unnoticed. | should-have |

## Resources and scale

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| NFR-PERF-020 | An idle server with the reference dataset **should** use less than 150 MiB of resident memory. | Gitea runs on a Raspberry Pi. A small footprint is a selling point for self-hosting and Rust makes it achievable. | should-have |
| NFR-PERF-021 | Startup time **should** stay under 5 s no matter how many repositories exist, so startup must not require scanning every repository. | Keeps restarts and upgrades fast. | should-have |
| NFR-PERF-022 | Blocking work (gix calls, filesystem walks, database calls without async drivers) **must** run off the async executor's worker threads. | A blocked worker thread stalls every other request on it. | must-have |
| NFR-PERF-023 | A single node **must** support the reference dataset while meeting every other target in this file. | Covers most self-hosted teams. | must-have |
| NFR-PERF-024 | Klotho **may** support scaling out across several nodes that share storage and a database. | GitLab (Gitaly cluster) and GitHub (Spokes) scale horizontally. This is only needed at larger scale. | nice-to-have |

## Caching and verification

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| NFR-PERF-030 | Results that can be derived from immutable git objects (diffs, rendered READMEs at a commit, archives) **should** be cached, keyed by object ID. | They never go stale, so caching them is always safe. | should-have |
| NFR-PERF-031 | The last commit for each tree entry **should** be cached per (commit, path). | Gitea's "last commit cache" exists because this is the slowest part of a tree view. | should-have |
| NFR-PERF-032 | A benchmark suite covering NFR-PERF-001, 002, 010 and 011 **should** run in CI and flag regressions greater than 10%. | Targets you don't measure get missed. | nice-to-have |

## Conflicts with the current implementation

Checked against commit `6b4895f`.

| Requirement | Current behaviour | Where |
|---|---|---|
| NFR-PERF-013 (streamed raw downloads) | **Conflict.** `read_blob` loads the whole blob into a `Vec<u8>`, which the handler then returns, so memory grows with file size. | [browse.rs:131-140](../../crates/git/src/browse.rs#L131-L140), [api.rs:102-111](../../crates/server/src/api.rs#L102-L111) |
| NFR-PERF-011 (listing independent of repo count) | **Conflict.** Every `GET /api/repos` reads and sorts the entire storage directory. | [store.rs:56-75](../../crates/git/src/store.rs#L56-L75) |

Met today: NFR-PERF-004 (request and response bodies are streamed through git's stdin and stdout), NFR-PERF-014 (the commit walk stops after `limit` commits), and NFR-PERF-022 (gix work runs in `spawn_blocking`).
