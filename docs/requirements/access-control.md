# Access control requirements

**Scope:** deciding *what* an identity may do once it is known. That covers roles, organisations and teams, how visibility is enforced, token scopes, branch and tag protection, and applying the same checks on every interface.

**Not in scope:** establishing identity ([auth.md](auth.md)) and choosing a repository's visibility setting ([repositories.md](repositories.md)).

## Model

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-ACL-001 | Each repository **must** grant one of at least three levels to a user or team: **read** (clone, browse), **write** (push, merge) and **admin** (settings, collaborators, delete). | Gitea/Forgejo use read/write/admin. GitHub and GitLab refine the same idea with more levels. | must-have |
| FR-ACL-002 | Klotho **should** add a **triage** level between read and write, allowing issue and PR management without push access. | GitHub has "Triage" and GitLab "Reporter". Lets people help maintain a project without being able to push code. | should-have |
| FR-ACL-003 | A user **must** have admin level on every repository they own personally. | Universal. | must-have |
| FR-ACL-004 | Instance administrators **must** be able to do anything on any repository and owner. | Universal. | must-have |
| FR-ACL-005 | Organisations **should** have members and teams, with repository access granted to teams. Organisation **owners** administer everything the organisation owns. | All four have orgs/groups with teams. | should-have |
| FR-ACL-006 | Repository administrators **should** be able to grant access to individual users outside the owning organisation ("outside collaborators"). | GitHub and Gitea collaborators, GitLab project members. | should-have |
| FR-ACL-007 | When a user has access through more than one path (direct grant, team, organisation), the highest level **must** apply. | All four. | must-have |

## Visibility enforcement

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-ACL-010 | Public repositories **must** be readable, and cloneable, without authentication unless FR-ACL-012 is enabled. | All four. | must-have |
| FR-ACL-011 | Private repositories **must** be invisible to anyone without at least read access. They must not appear in listings, search, API results or counts. | All four. | must-have |
| FR-ACL-012 | Administrators **should** be able to require sign-in for everything, including public repositories. | Gitea `REQUIRE_SIGNIN_VIEW` and GitLab restricted visibility levels. Suits company-internal instances. | should-have |
| FR-ACL-013 | A request for a private repository by someone without read access **must** get exactly the same response as a request for a repository that doesn't exist: same status, same body, and a similar response time. | GitHub returns 404 for both, so private names can't be discovered. | must-have |

## Tokens and keys

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-ACL-020 | A deploy key **must** grant access only to its one repository, at the level it was given (read-only by default). | All four. | must-have |
| FR-ACL-030 | A personal access token's effective permissions **must** be the intersection of its scopes and its owner's current permissions. | GitHub, GitLab and Gitea. A token can never do more than its owner, and it stops working when the owner loses access. | must-have |
| FR-ACL-031 | Tokens **should** support scopes at least for repository read, repository write, admin actions and user profile. They **may** also be restricted to specific repositories. | GitHub fine-grained PATs restrict by repository. GitLab and Gitea have scopes. | should-have |

## Protected refs

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-ACL-040 | Repository administrators **should** be able to protect branches by pattern. By default a protected branch rejects force-pushes and deletion from anyone. | All four. It protects the history of `main`. | should-have |
| FR-ACL-041 | Branch protection **should** support: limiting who may push; requiring changes to arrive through a pull request; a minimum number of approving reviews; and required status checks passing. | GitHub, GitLab and Gitea. It is the enforcement point for CI results from Lachesis. | should-have |
| FR-ACL-042 | Repository administrators **may** protect tags by pattern, so only allowed users can create, move or delete them. | GitLab protected tags, GitHub tag rulesets, Gitea protected tags. It protects release tags that Atropos deploys from. | nice-to-have |
| FR-ACL-043 | Pushes to archived repositories **must** be rejected for everyone. | All four. | must-have |

## Consistency

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-ACL-050 | The same permission decision **must** be made for the same identity and action on every interface: web, API, git over HTTP and git over SSH. It should come from one shared authorisation function. | Checks written separately for each interface drift apart. Gitea has had security bugs where API and web permission checks disagreed. | must-have |
| FR-ACL-051 | Permission changes, such as removing a team member or revoking a grant, **must** take effect on the next request, with no caching delay longer than 60 seconds. | Revoking access needs to happen quickly. | must-have |

## Conflicts with the current implementation

Checked against commit `6b4895f`.

| Requirement | Current behaviour | Where |
|---|---|---|
| FR-ACL-001 (push requires write) | **Conflict (critical).** `git-receive-pack` is served to any anonymous client, so anyone who can reach the port can push, overwrite branches or delete refs. | [git_http.rs:31-38](../../crates/server/src/git_http.rs#L31-L38) |
| FR-ACL-011 (private repos invisible) | **Conflict.** There is no visibility setting, so every repository is effectively public and is listed by `GET /api/v1/users/{username}/repos`. | [api.rs](../../crates/server/src/api.rs) (`user_repos`) |

The default bind address of `127.0.0.1` limits the exposure while there is no access control. Changing `KLOTHO_ADDR` to a public interface exposes every repository to writes.
