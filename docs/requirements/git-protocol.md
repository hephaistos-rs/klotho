# Git transport requirements

**Scope:** how git clients talk to Klotho. That covers the HTTP and SSH transports, protocol versions, clone and fetch features, Git LFS, archives and the server-side hook points during a push.

**Not in scope:** who may fetch or push ([access-control.md](access-control.md)), how clients prove their identity ([auth.md](auth.md)), how URLs resolve to repositories ([naming.md](naming.md)), and what happens after a push ([integrations.md](integrations.md)).

## Transports

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-GIT-001 | Klotho **must** support clone, fetch and push over git's smart HTTP protocol. | All four. It works through proxies and firewalls with no extra setup. | must-have |
| FR-GIT-002 | Klotho **must** support clone, fetch and push over SSH. | All four. Many developers prefer key-based SSH remotes. | must-have |
| FR-GIT-003 | SSH **should** be available through a built-in SSH server, and **may** alternatively integrate with the system OpenSSH through `AuthorizedKeysCommand`. | Gitea/Forgejo offer both. The built-in server keeps installs to a single binary; OpenSSH integration suits hosts that already run sshd on port 22. | should-have |
| FR-GIT-004 | Requests for the dumb HTTP protocol **must** get a 4xx response with a message explaining that only smart HTTP is supported. | None of the four serve dumb HTTP for push, and it bypasses access checks on individual objects. | must-have |
| FR-GIT-005 | Git protocol version 2 **must** be supported over both transports, with fallback to v0/v1 for older clients. | Git has used v2 by default since 2.26. It reduces how many refs are advertised on large repositories. | must-have |
| FR-GIT-006 | Gzip-compressed request bodies **must** be accepted. | Git gzips large fetch negotiations over HTTP. | must-have |

## Fetch features

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-GIT-010 | Shallow clones (`--depth`, `--shallow-since`) **must** work. | CI systems (Lachesis) rely on shallow clones. All four support them. | must-have |
| FR-GIT-011 | Partial clones (`--filter=blob:none`, `tree:0`) **should** work. | GitHub and GitLab support them. They make large repositories practical. | should-have |
| FR-GIT-012 | Users **should** be able to download an archive (`.zip`, `.tar.gz`) of any commit, branch or tag over HTTP. | All four. Used by package managers and release consumers. | should-have |

## Push features

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-GIT-020 | Before any ref is updated, a push **must** pass through a decision point that can reject the whole push or individual refs, with a message shown to the user. This is a pre-receive step. | All four use it to enforce branch protection, quotas and archive state. | must-have |
| FR-GIT-021 | After refs are updated, a push **must** produce one event listing every updated ref as `(ref, old id, new id)`. This is a post-receive step. | All four drive webhooks, CI and UI caches from this event. | must-have |
| FR-GIT-022 | Atomic pushes (`git push --atomic`) **should** be honoured. | Part of git's push protocol. GitHub and GitLab expose it. | should-have |
| FR-GIT-023 | Push options (`git push -o key=value`) **may** be passed to the post-receive event. | GitLab uses them, for example to create a merge request or skip CI. | nice-to-have |
| FR-GIT-024 | Pushes to ref namespaces the server manages itself, such as `refs/pull/*`, **must** be rejected. | GitHub makes `refs/pull/*` read-only so PR heads can't be forged. | must-have |
| FR-GIT-025 | The post-push output **may** include a link to open a pull request for a newly pushed branch. | GitHub, GitLab and Gitea print "Create a pull request" links. | nice-to-have |

## Large files and object formats

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-GIT-030 | Git LFS (batch API and basic transfer adapter) **should** be supported. | All four. It is expected for game, ML and media repositories. | should-have |
| FR-GIT-031 | Repositories using the SHA-256 object format **may** be supported. | Gitea supports SHA-256 repositories and GitLab has experimental support. | nice-to-have |

## Conflicts with the current implementation

Checked after Phase 1b (2026-10-02). Everything below is served by Klotho's own engine; the `git` program is no longer involved ([ADR 0004](../decisions/0004-native-git-transport.md)).

| Requirement | Current behaviour | Where |
|---|---|---|
| FR-GIT-011 (partial clones) | **Missing.** Filters aren't advertised, so `--filter` clones fall back to full clones (Phase 1b, step 5). | [select.rs](../../crates/git/src/protocol/select.rs) |
| FR-GIT-020 (pre-receive) | **Partial.** `ReceiveHooks::pre_receive` runs before any ref changes and can reject the push with a message the client prints, but only as a whole, not ref by ref. It accepts everything until Phase 2 fills it in. | [receive.rs](../../crates/git/src/protocol/receive.rs), [git_http.rs](../../crates/server/src/git_http.rs) (`PushHooks`) |
| FR-GIT-021 (post-receive event) | **Partial.** `ReceiveHooks::post_receive` gets every applied `(ref, old, new)`, but only logs them; the event and its consumers come later. | same |
| FR-GIT-024 (protect server-managed refs) | **Not enforced.** Any `refs/` namespace can be written. It only becomes a real conflict once Klotho manages refs such as PR heads; `pre_receive` is where the check goes. | same |

Met today:
- FR-GIT-001: clone, fetch and push over smart HTTP.
- FR-GIT-004: 403 with a message for dumb HTTP.
- FR-GIT-005: protocol v2, with v0 and v1 for older clients and for every push (git has no v2 push).
- FR-GIT-006: gzip-compressed requests.
- FR-GIT-010: shallow clones: `--depth`, `--deepen`, `--shallow-since`, `--shallow-exclude`, `--unshallow`, and fetching into a shallow clone.
- FR-GIT-022: `--atomic` pushes, in one ref transaction.
- FR-GIT-023: push options reach both hook points (unused so far).
