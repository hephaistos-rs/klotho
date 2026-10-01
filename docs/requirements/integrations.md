# Integration requirements

**Scope:** how Klotho tells other systems what happened and takes results back from them. That covers webhooks, commit statuses and checks, the hand-off to Lachesis (CI) and Atropos (deployment), package registries and federation.

**Not in scope:** Klotho acting as an OAuth2 provider ([auth.md](auth.md), FR-AUTH-035), and pull or push mirrors ([repositories.md](repositories.md)).

## Webhooks

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-INT-001 | Repository and organisation administrators **must** be able to configure webhooks that POST a JSON payload to a URL for chosen events. The events include at least push, tag create/delete, pull request, issue, comment, release, and repository create/delete/rename. | All four. It is the general way to integrate with anything. | must-have |
| FR-INT-002 | Each delivery **must** be signed with HMAC-SHA256 of the body using a per-hook secret, sent in a header, e.g. `X-Klotho-Signature-256: sha256=<hex>`. | GitHub `X-Hub-Signature-256` and Gitea `X-Gitea-Signature`. Receivers can verify where a payload came from. | must-have |
| FR-INT-003 | Every delivery **must** carry a unique delivery ID and an event-type header. | GitHub `X-GitHub-Delivery`/`X-GitHub-Event`. Receivers can drop duplicates and route by event. | must-have |
| FR-INT-004 | Failed deliveries (a network error or a non-2xx response) **should** be retried with exponential backoff, at least 3 times over at least 1 hour. | GitLab retries webhooks and Gitea queues them. Short outages at the receiver shouldn't lose events. | should-have |
| FR-INT-005 | Administrators **should** be able to see recent deliveries with their request, response and timing, and to redeliver one manually. | GitHub, GitLab and Gitea delivery logs. Essential for debugging. | should-have |
| FR-INT-006 | Webhook payloads for push and pull request events **should** match the field names of Gitea's payloads (which are themselves close to GitHub's) wherever the concepts match. | Existing tools that accept Gitea or GitHub webhooks (Woodpecker, Renovate, chat bots) then work with no or minimal changes. | should-have |
| FR-INT-007 | Klotho **may** offer instance-wide ("system") webhooks configured by administrators. | GitLab system hooks and Gitea system webhooks. | nice-to-have |

## CI and deployment

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-INT-010 | External systems **must** be able to attach commit statuses (`pending`, `success`, `failure`, `error`) to a commit, each with a context name, description and target URL. | GitHub, GitLab and Gitea commit status APIs. This is how Lachesis reports results. Branch protection uses these statuses (FR-ACL-041). | must-have |
| FR-INT-011 | The combined status of the latest commit **must** be shown on pull requests and branch listings. | All four. | must-have |
| FR-INT-012 | Klotho **should** notify Lachesis of pushes and pull request events through the same webhook mechanism available to third parties, not a private channel. | Keeps the Moirai loosely coupled, as the README intends. Any other CI, such as Woodpecker, can be swapped in the same way. | should-have |
| FR-INT-013 | Klotho **should** record deployments of a commit to a named environment, with a status and URL, as reported by Atropos or another deployer, and show them on the repository. | GitHub Deployments API and GitLab Environments. It makes the path from commit to running app visible. | should-have |

## Ecosystem

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-INT-020 | Klotho **may** host package registries (OCI containers first, then others). | GitHub Packages, GitLab and Gitea/Forgejo package registries. Atropos can pull images from the same instance. | nice-to-have |
| FR-INT-021 | Klotho **may** federate with other forges over ActivityPub/ForgeFed, e.g. to star repositories on another instance or follow users there. | Forgejo is implementing ForgeFed federation. | nice-to-have |

## Conflicts with the current implementation

None: integrations haven't been started (commit `6b4895f`). Note that FR-INT-001 depends on the post-receive event (FR-GIT-021), which is currently missing.
