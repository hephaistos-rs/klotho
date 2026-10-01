# Repository lifecycle requirements

**Scope:** the operations that create, change and remove repositories and their settings. That covers creation, the default branch, metadata, visibility, rename, transfer, archive, delete, fork, mirror, import and adoption.

**Not in scope:** name rules and redirects ([naming.md](naming.md)), who may perform these operations ([access-control.md](access-control.md)), and on-disk layout ([storage.md](storage.md)).

## Creation

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-REPO-001 | An authorised user **must** be able to create an empty bare repository under an owner they can write to. | Core function of every forge. | must-have |
| FR-REPO-002 | A new repository's HEAD **must** point at the instance's default branch name, which defaults to `main`. | GitHub, GitLab and Gitea all default to `main`. | must-have |
| FR-REPO-003 | Administrators **should** be able to set the instance-wide default branch name, and owners **may** override it for their own new repositories. | GitHub (user/org setting), GitLab (instance and group setting), Gitea (`DEFAULT_BRANCH`). | should-have |
| FR-REPO-004 | Creation **may** optionally add an initial commit with a README, a `.gitignore` template and a licence. | All four offer it, which saves a round-trip for new projects. | nice-to-have |
| FR-REPO-005 | Creation **may** start from a template repository, copying its files (and optionally labels and settings) into a fresh history. | GitHub, GitLab and Gitea template repositories. | nice-to-have |

## Settings and metadata

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-REPO-010 | Each repository **must** have a visibility of `public` or `private`, chosen at creation and changeable later. | All four. What visibility means for access is defined in [access-control.md](access-control.md). | must-have |
| FR-REPO-011 | Each repository **may** also offer an `internal` visibility, meaning readable by any signed-in user. | GitLab and GitHub Enterprise use this for inner-source. | nice-to-have |
| FR-REPO-012 | Repositories **should** have an editable description, website URL and topic list. | All four. Used by search and listings. | should-have |
| FR-REPO-013 | Repository administrators **must** be able to change the default branch to any existing branch. | All four. | must-have |
| FR-REPO-014 | The size of each repository on disk **should** be reported in the UI and API. | All four. Admins need it before quotas (FR-STOR-050) are useful. | should-have |

## Rename, transfer, archive, delete

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-REPO-020 | Repository administrators **must** be able to rename a repository. | All four. Redirects after a rename are covered in FR-NAME-050 and following. | must-have |
| FR-REPO-021 | Repository administrators **should** be able to transfer a repository to another owner they administer. Transfers to an owner they don't administer **should** need that owner to accept. | GitHub asks the receiving owner to accept. GitLab and Gitea require permission on the target. | should-have |
| FR-REPO-022 | Repository administrators **should** be able to archive a repository. It then becomes read-only for every protocol (pushes rejected, no new issues or PRs), and can be unarchived later. | All four. Keeps history available without implying the project is maintained. | should-have |
| FR-REPO-023 | Deleting a repository **must** require explicit confirmation, e.g. by typing its full name, in the UI. | GitHub and GitLab ask for the name to be typed, which prevents accidental deletes. | must-have |
| FR-REPO-024 | Deleted repositories **should** be recoverable by an administrator for a configurable period (default 7 days) before being purged. | GitHub lets you restore within 90 days; GitLab has delayed project deletion. | should-have |

## Forks, mirrors, import, adoption

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-REPO-030 | Users **should** be able to fork a repository they can read into an owner they can write to. The fork records its upstream. | All four. Forking is how outside contributors propose changes (see [collaboration.md](collaboration.md)). | should-have |
| FR-REPO-031 | Repositories **should** be creatable as pull mirrors of an external git URL, synced on a configurable interval and on demand. | GitLab and Gitea/Forgejo pull mirrors. Useful for self-hosters who follow upstream projects. | should-have |
| FR-REPO-032 | Repositories **may** have push mirrors that forward every push to an external remote. | GitLab and Gitea push mirrors, e.g. keeping a GitHub copy up to date. | nice-to-have |
| FR-REPO-033 | Users **should** be able to import a repository's git data from any git URL, with optional credentials. | All four import from a URL. | should-have |
| FR-REPO-034 | Imports from GitHub, GitLab, Gitea and Forgejo **may** also bring across issues, pull requests, labels, milestones and releases. | Gitea/Forgejo "migrations" and GitLab importers. This lowers the cost of switching to Klotho. | nice-to-have |
| FR-REPO-035 | Administrators **should** be able to adopt a bare repository found under the storage root, registering it under an owner and name of their choice. They **should** also be able to delete it. | Gitea "adopt unadopted repositories". This pairs with FR-STOR-020, which reports such directories. | should-have |

## Listing

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-REPO-040 | Users **must** be able to list the repositories visible to them, filtered by owner and sorted by name or by last update. | All four. How results are paginated is set by [api.md](api.md). | must-have |

## Conflicts with the current implementation

Checked after Phase 1 (2026-10-01).

| Requirement | Current behaviour | Where |
|---|---|---|
| FR-REPO-001 (only authorised users create) | **Conflict.** Repositories are created through `POST /api/v1/admin/users/{username}/repos`, which has no authentication until Phase 2, so anyone who can reach the server can create them. | [api.rs](../../crates/server/src/api.rs) (`create_repo`) |
| FR-REPO-003 (configurable default branch) | **Conflict.** The default branch is the compile-time constant `main`. It matches FR-REPO-002 but can't be changed (Phase 6). | [store.rs](../../crates/git/src/store.rs) (`DEFAULT_BRANCH`) |

Met today: FR-REPO-002 (HEAD points at `main`) and FR-REPO-035 (unadopted repositories can be adopted under an owner, moving them to their ID-based path, or deleted; through `klotho admin` and the API).
