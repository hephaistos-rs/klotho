# Klotho

A self-hosted git forge in Rust. The build order is in [ROADMAP.md](ROADMAP.md), the requirements in [docs/requirements/](docs/requirements/), and the decisions in [docs/decisions/](docs/decisions/).

## Rules

- **Klotho never runs the `git` program or any other subprocess** ([ADR 0004](docs/decisions/0004-native-git-transport.md)). All git work is our own code on gitoxide crates (`gix`, `gix-pack`, `gix-ref`, …). Tests may run the `git` client, and `xtask` may run tools.
- **No legacy or compatibility code.** Klotho is unreleased: no users, no old clients, no old databases to support. Change APIs, config keys and code freely, without shims.
- **Never edit a committed migration** in `crates/core/migrations/`. Add a new one (see the `new-migration` skill).
- **Never use Python.** Use the built-in tools, shell and Rust.
- **Leave the default address `0.0.0.0:3000` alone.** It's deliberate.
- **Never write the maintainer's email address** into code, tests, fixtures, docs or commits, and never send it to a service.
- **Don't push and don't change remotes.**
- **Access is decided in one place:** `klotho_core::access::authorize()`, reached through `Core::repo_for`. "Not visible" must look exactly like "doesn't exist" (`RepoNotFound`). From Phase 7, every ref update made for a user goes through one `check_ref_update()`.
- **UI and API move together.** A page action gets its API endpoint in the same change, and both call the same `klotho-core` service.
- **Every change names its requirement IDs** (e.g. FR-AUTH-043) in the commit message, and updates the roadmap boxes and the conflict tables in `docs/requirements/*.md`.

## Layout

| Crate | What's in it |
|---|---|
| `crates/core` | `Core`: database (sqlx, SQLite), services, `authorize()`, accounts, config |
| `crates/git` | Repository store and the native protocol engine (`protocol/`: pkt-line, v0/v1/v2 upload-pack, receive-pack) |
| `crates/server` | axum: `/api/v1` (Bearer tokens only), git smart HTTP (Basic auth, token as password), mounts the web UI |
| `crates/web` | Topcoat pages, server-rendered, no JavaScript needed |
| `crates/klotho` | The binary and the `klotho admin …` CLI |
| `xtask` | `cargo xtask dev`, `dist`, `sqlx-prepare` |

End-to-end tests live in `crates/server/tests/` and drive the real `git` client against the real server (`tests/common/mod.rs` has the helpers: `state()`, `token()`, `Running::url()`, `git_command`).

## Checks

Run all of these before calling anything done:

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
```

After adding or changing a `sqlx::query!`, run `cargo xtask sqlx-prepare` and commit `.sqlx/`, including the deleted files. `cargo xtask dist` builds the release binary with the assets embedded.

## Environment

Development is on Windows (Git Bash and PowerShell); production is Linux. Both must work (NFR-OPS-002). Watch for:
- `File::sync_all` needs a handle opened for writing on Windows.
- Directory fsync is unix only.
- Open files can't be renamed or deleted on Windows.

For the web UI, see the `topcoat-ui` skill before writing pages.
