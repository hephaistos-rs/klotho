# Operations requirements

**Scope:** installing, configuring, running, observing and upgrading Klotho. That covers packaging, supported platforms, external dependencies, configuration, logs, metrics, health checks, shutdown, schema migrations and admin tooling.

**Not in scope:** backup and restore ([storage.md](storage.md)), and TLS and proxy trust ([security.md](security.md)).

## Packaging and platforms

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| NFR-OPS-001 | Klotho **must** ship as a single executable. Its only runtime dependency **must** be a `git` binary. | Gitea/Forgejo ship a single binary, which is much of their appeal for self-hosting. | must-have |
| NFR-OPS-002 | Linux on x86_64 and aarch64 **must** be supported for production use. Windows and macOS **should** be supported for development and small installs. | Linux is where forges get deployed. Klotho is developed on Windows, so it has to keep working there. | must-have |
| NFR-OPS-003 | Klotho **must** check the installed `git` version at startup and refuse to start, with a clear message, if it is older than the documented minimum. | Gitea checks the git version at startup. Failing on the first clone is much harder to diagnose. | should-have |
| NFR-OPS-004 | The project **should** publish an official container image and an example systemd unit. | Gitea, Forgejo and GitLab all do. These are the two most common ways to deploy. | should-have |
| NFR-OPS-005 | Building Klotho from source **must not** require Node.js or any toolchain other than Rust, git and the `topcoat` CLI. Release builds **must** embed every web asset (stylesheets, scripts, fonts, icons) in the binary. | Keeps NFR-OPS-001 (single binary) true with the Topcoat UI, whose asset bundle is written to disk after the build by default ([ADR 0002](../decisions/0002-topcoat-ui.md)). | must-have |

## Configuration

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| NFR-OPS-010 | Configuration **must** come from a file (TOML), and every setting in it **must** be overridable by an environment variable with a documented, predictable name, e.g. `KLOTHO_SERVER__ADDR`. | Gitea (`app.ini` plus `GITEA__section__KEY` variables) and GitLab (`gitlab.rb`). Files suit hosts; environment variables suit containers. | must-have |
| NFR-OPS-011 | Configuration **must** be fully validated at startup. Unknown keys and invalid values **must** stop startup with an error naming the key. | Otherwise a typo silently falls back to a default. | must-have |
| NFR-OPS-012 | By default the server **must** listen only on loopback. | A safe default until an admin has configured authentication and TLS. | must-have |
| NFR-OPS-013 | Every setting **must** be documented, with its default, in one reference page. | Gitea's "Config Cheat Sheet". | should-have |

## Observability

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| NFR-OPS-020 | Logs **must** be structured, with levels, and **should** be available as JSON. Every request **should** get a request ID that appears in all its log lines and in an `X-Request-Id` response header. | GitLab uses structured JSON logs and correlation IDs. They make logs searchable and let you tie a user's report to a log line. | should-have |
| NFR-OPS-021 | Klotho **should** expose Prometheus metrics: request counts and latencies by route, git operations by type and result, active subprocesses, queue depth and storage usage. | Gitea `/metrics` and GitLab Prometheus. | should-have |
| NFR-OPS-022 | Klotho **must** expose unauthenticated liveness and readiness endpoints under `/-/`. Readiness is true only when storage and the database are reachable. | GitLab `/-/health`, `/-/readiness`. Needed by container orchestrators and load balancers. | should-have |
| NFR-OPS-023 | Logs **must never** include passwords, tokens, session cookies or `Authorization` headers. | Standard. Log files are usually less protected than the database. | must-have |

## Lifecycle

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| NFR-OPS-030 | On SIGTERM or Ctrl-C the server **must** stop accepting new connections and let running git operations finish, up to a configurable grace period (default 30 s), before exiting. | A push cut off mid-transfer fails for the user and wastes their upload. Gitea uses graceful restarts. | must-have |
| NFR-OPS-031 | Database schema migrations **must** run automatically at startup, be forward-only and run inside transactions. The server **must** refuse to start on a schema newer than itself. | Gitea and GitLab migrate on upgrade. Refusing a newer schema prevents damage from an accidental downgrade. | must-have |
| NFR-OPS-032 | Upgrading between consecutive minor versions **must** need nothing more than replacing the binary and restarting. Anything more **must** be written in the release notes. | Gitea and Forgejo have simple upgrades, which matters to self-hosters who upgrade rarely. | must-have |
| NFR-OPS-033 | Klotho **should** provide an admin CLI for tasks that must work while the web UI is unavailable or before an admin account exists. These include: create or modify users, reset passwords, adopt repositories, run backups, check consistency and regenerate SSH `authorized_keys`. | `gitea admin` and `forgejo` CLIs, `gitlab-rake`. | should-have |
| NFR-OPS-034 | The README **must** document installation, the minimum git version, and the first-run steps. | The README currently has TODO placeholders for all three. | should-have |

## Conflicts with the current implementation

Checked against commit `6b4895f`.

| Requirement | Current behaviour | Where |
|---|---|---|
| NFR-OPS-003 (git version check) | **Conflict.** The server never checks that `git` exists or which version it is. A missing binary only shows up on the first clone, as a 500. | [git_http.rs:62-73](../../crates/server/src/git_http.rs#L62-L73), [main.rs](../../crates/klotho/src/main.rs) |
| NFR-OPS-010, NFR-OPS-011 (config file, validation) | **Partial.** Configuration is two environment variables (`KLOTHO_ADDR`, `KLOTHO_REPOS`) with no config file, as the existing TODO notes. Invalid addresses are rejected, but the storage path is taken as given. | [main.rs:14-18](../../crates/klotho/src/main.rs#L14-L18) |
| NFR-OPS-020 (structured logs, request IDs) | **Partial.** `tracing` gives levelled logs with an `EnvFilter`, but the output is plain text and there are no request IDs. | [main.rs:10-12](../../crates/klotho/src/main.rs#L10-L12) |
| NFR-OPS-030 (graceful shutdown) | **Conflict.** `axum::serve` runs without `with_graceful_shutdown`, so Ctrl-C or SIGTERM ends the process immediately and cuts off in-flight pushes. | [main.rs:23](../../crates/klotho/src/main.rs#L23) |

Met today: NFR-OPS-012 (default `127.0.0.1:3000`).
