# ADR 0004: Git in-process with gitoxide, no `git` program

- **Status:** accepted
- **Date:** 2026-10-01
- **Supersedes:** the *Git* section of [ADR 0001](0001-stack.md) ("gix for reading, the `git` program for the wire protocol").

## Context

ADR 0001 split git work in two: gix (gitoxide) reads repositories, and the `git` program, run as `git upload-pack` / `git receive-pack` subprocesses, serves clone, fetch and push. The roadmap then planned more subprocesses: hook scripts calling back into Klotho, `git fetch` for imports and mirrors, `git blame`, `git merge-tree` for pull requests, and `git maintenance` / `git fsck` for housekeeping.

That makes `git` a runtime dependency of what is otherwise a single Rust binary, and it brings work that exists only because of the subprocesses: a version check at startup, concurrency caps and kill-on-disconnect for child processes, a scrubbed environment, argument-injection rules, and a hook protocol between git and the server. We don't want any of that. Klotho should do its git work itself.

What gitoxide provides today (checked against `gix` 0.87, `gix-protocol` 0.65, `gix-pack` 0.74):

| Need | gitoxide |
|---|---|
| Reading objects, refs, trees, diffs | ✅ |
| pkt-line encoding and decoding | ✅ `gix-packetline` |
| Building a pack to send | ✅ `gix-pack` `data::output` (count objects, then encode entries). It reuses deltas already in packs but doesn't search for new ones |
| Indexing a received pack, including thin packs | ✅ `gix-pack` `Bundle::write_to_directory` |
| Atomic ref updates with locks | ✅ `gix-ref` transactions |
| Blame, merges, merge bases, archives | ✅ `gix-blame`, `gix-merge`, `gix-revision`, `gix-archive` |
| Fetching from another server (imports, mirrors) | ✅ the `gix` client |
| **Server side of the protocol**: ref advertisement, `ls-refs`, want/have negotiation, the v2 `fetch` command, `receive-pack` commands and report-status | ❌ client side only |
| Shallow-clone boundaries, partial-clone filters, as a server | ❌ |
| Repack / gc, full `fsck` | ❌ mostly missing |

## Decision

**Klotho never runs the `git` program.** All git work happens in-process through gitoxide's crates, and the parts gitoxide doesn't have, Klotho implements in `klotho-git`.

### The protocol engine

A `protocol` module in `klotho-git`, with no HTTP or SSH in it. It takes a repository and a byte stream in each direction, so the same engine serves:

- **HTTP** (stateless: each request carries the whole negotiation state) from `klotho-server`, and
- **SSH** (stateful, one long conversation) from `klotho-ssh`.

It covers:

- **upload-pack** (clone, fetch): protocol v2 (`ls-refs`, `fetch`) and v0/v1 (ref advertisement, want/have with `multi_ack_detailed` and `no-done`), `side-band-64k` progress, and later shallow (`deepen`, `deepen-since`, `deepen-not`) and filters (`blob:none`, `tree:0`).
- **receive-pack** (push): v0/v1 only, because git has no v2 push. Commands, `report-status`, `atomic`, `push-options`, and thin packs.
- **Pack generation** with `gix-pack`'s output pipeline. Packing is CPU-bound and blocking, so it runs on the blocking pool behind a semaphore that limits how many packs are built at once (the intent of NFR-SEC-022, without processes).

### Pushes: quarantine, checks, then refs

1. The incoming pack is written and indexed in a **quarantine directory** inside the repository, not in `objects/pack`, as git itself does.
2. Klotho checks it: every object it needs is present (connectivity), the objects are well-formed, and size limits hold.
3. **Pre-receive is a function call** (FR-GIT-020): `check_ref_update()` for each ref, with the quarantined objects readable. No hook scripts, no callback protocol, no `klotho hook` subcommand.
4. On success, the pack moves into `objects/pack`, is `fsync`ed, and refs are updated in one `gix-ref` transaction (all or nothing for `--atomic`). Klotho does the `fsync`s itself now that `core.fsync` no longer applies (NFR-STOR-001).
5. **Post-receive is a function call** (FR-GIT-021) that emits the push event.

### Everything else

| Was planned as | Now |
|---|---|
| `git fetch` for imports and pull mirrors (P6) | The `gix` client, with our SSRF guard on its connections |
| `git blame --porcelain` (P6) | `gix-blame` |
| `git merge-base --is-ancestor` for force-push detection (P7) | gix merge base, in-process, on the quarantined objects |
| `git merge-tree` / `commit-tree` for PR merges (P9) | `gix-merge` |
| `git archive` (P4) | `gix-archive` |
| `git maintenance`, `git fsck` (P10) | Our own repack built on `gix-pack`, and connectivity plus object checks. These are later work, not blockers |
| Repository config `core.fsync`, `receive.fsckObjects` (P4) | Done by Klotho's own write path |

### Until the engine is ready

*Done 2026-10-02; see the update at the end.* The current subprocess transport (`crates/server/src/git_http.rs`) and the startup git version check stay, **unchanged and isolated**, until the native engine passes the end-to-end test suite. The PR that switches to the native engine deletes both. That keeps Klotho usable while the engine is built, with a clear end state.

### Testing

- **End-to-end tests keep using the real `git` client** against the real Klotho binary. That's a test-time dependency only, and it's the point: the client is the reference implementation we must interoperate with. `cargo xtask ci` runs it; Linux and Windows both have to pass (see NFR-OPS-002 in operations.md for how Linux is covered today).
- The protocol parsers handle untrusted input from the network, so they get fuzz targets (`cargo fuzz`) from the start.
- A test matrix covers protocol v0, v1 and v2, shallow and partial clones, thin packs, atomic pushes, and pushes rejected by pre-receive (the client must show our message).

## Alternatives considered

| Option | Why not |
|---|---|
| **Keep the `git` program (ADR 0001)** | It works and is battle-tested, and Gitea, Forgejo and GitLab all do it. But it's the runtime dependency and the subprocess machinery this ADR exists to remove, and every hook becomes a round trip between processes. |
| **libgit2 (`git2` crate)** | A C library, and it doesn't implement the server side of the protocol either. |
| **Wait for gitoxide to add server support** | It's not on a timeline we can plan around. Building the engine as a self-contained module in `klotho-git` means it could be offered upstream later. |

## Consequences

**Good:**
- One binary with **no** runtime dependencies (NFR-OPS-001 strengthened).
- Hooks are function calls. Phase 4's hardest part disappears.
- No subprocess caps, environment scrubbing or argument-injection rules to get right. Limits, timeouts and metrics apply to our own code, which we can measure.
- Identical behaviour on Linux, Windows and macOS: no Git for Windows path quirks.

**Bad:**
- **We own protocol correctness and security.** Parsing untrusted network input is now our code, so fuzzing is mandatory, not optional.
- **Performance risk.** `gix-pack` reuses existing deltas but doesn't compute new ones, and there are no reachability bitmaps, so clones of loosely packed or very large repositories may be slower and larger than with `git upload-pack`. NFR-PERF-001 (within 110% of `git http-backend`) will measure this. Our own repack (P10) is the fix.
- **Repack/gc and full `fsck` don't exist in gitoxide**, so housekeeping is our own work.
- **New protocol features arrive later** than in git itself.
- **A large piece of work** sits early in the roadmap (the new Phase 1b).

**Requirement changes**, applied in the PR that removes the subprocess transport:
- **Reworded:** NFR-OPS-001 (no runtime dependencies at all); NFR-SEC-022 and NFR-SEC-023 (limits on concurrent pack generation and per-request time instead of subprocesses); NFR-STOR-001 (Klotho fsyncs packs and refs itself).
- **Withdrawn:** NFR-OPS-003 (git version check), NFR-SEC-040 and NFR-SEC-041 (subprocess arguments and environment), since Klotho starts no subprocesses.

**Updated 2026-10-02, after step 1 (v2 fetch).**
- pkt-lines are a small module of our own, not `gix-packetline`. The encoding is trivial, and owning it means our own limits on untrusted input.
- Pack writing is behind `gix-pack`'s `generate` feature, which `gix` doesn't enable. `klotho-git` depends on `gix-pack` at the exact version `gix` uses, so Cargo turns the feature on for that same crate. Keep the two in step on every gix upgrade.

**Updated 2026-10-02, switch-over.** The engine serves everything: v2 and v0/v1 fetch, shallow fetches, and push. The subprocess transport, the startup git version check and `GitVersion` are deleted, and the requirement changes above are applied. Where the build differs from this ADR:
- **The push quarantine is under the store's `.tmp/`**, not inside the repository. It's on the same filesystem, so the checked pack is still moved with a rename, and `.tmp/` is already emptied at startup, which cleans up after a crash.
- **Pre-receive and post-receive are one trait**, `protocol::ReceiveHooks`, not a per-ref `check_ref_update()`. Pre-receive rejects the whole push for now; per-ref rejection comes with branch protection.
- **Fetch packs are never thin.** `gix-pack` can't restrict thin-pack bases to objects the client is known to have, and a wrong base breaks shallow clones. Received thin packs are fine. This adds to the performance risk above.
- **Pack indexing needs `gix-pack`'s `streaming-input` feature**, enabled the same way as `generate`.
- Still open: partial clone filters, the cap on concurrent packs, size limits on pushes, fuzz targets, and the NFR-PERF-001 measurement.

**Revisit if:** the engine can't reach the NFR-PERF-001 target after our own repack exists, or gitoxide gains a maintained server side we could adopt instead.
