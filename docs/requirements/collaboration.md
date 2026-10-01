# Collaboration requirements

**Scope:** how people work together on a repository: pull requests, code review, merging, issues, releases and notifications.

**Not in scope:** who can merge or review ([access-control.md](access-control.md)), CI status reporting and webhooks ([integrations.md](integrations.md)), and file and commit browsing ([web-ui.md](web-ui.md)).

## Pull requests

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-COLLAB-001 | Users **must** be able to open a pull request from a branch in the same repository or in a fork into a branch of the upstream repository. | Core workflow on all four (GitLab calls them merge requests). | must-have |
| FR-COLLAB-002 | A pull request **must** show its description, commits, the combined diff against the merge base, and whether it can be merged without conflicts. | All four. | must-have |
| FR-COLLAB-003 | Each pull request's head **must** be exposed as a read-only ref, `refs/pull/<n>/head`, so it can be fetched even from a fork. | GitHub `refs/pull/*`, GitLab `refs/merge-requests/*`, Gitea `refs/pull/*`. CI (Lachesis) fetches PR heads this way. | must-have |
| FR-COLLAB-004 | Pull requests **must** support merge commit, squash and rebase merge strategies. Repository administrators **should** be able to limit which ones are allowed. | All four. | must-have |
| FR-COLLAB-005 | Pull requests **may** be opened as drafts, which can't be merged until marked ready. | GitHub drafts, GitLab and Gitea "WIP/Draft". | nice-to-have |
| FR-COLLAB-006 | After a merge, the source branch **may** be deleted automatically if the author opted in. | All four. | nice-to-have |
| FR-COLLAB-007 | A pull request **may** be set to merge automatically once its required checks and reviews pass. | GitHub auto-merge, GitLab "merge when pipeline succeeds", Gitea "auto merge". | nice-to-have |

## Review

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-COLLAB-010 | Reviewers **must** be able to comment on individual diff lines and line ranges, and to submit a review as *comment*, *approve* or *request changes*. | All four. | must-have |
| FR-COLLAB-011 | Line comments **must** stay attached to their original commit and be shown as outdated when later pushes change those lines. | All four keep review context across force-pushes. | must-have |
| FR-COLLAB-012 | Reviewers **may** suggest a change inline that the author can apply as a commit. | GitHub and GitLab suggestions; Gitea is adding them. | nice-to-have |
| FR-COLLAB-013 | A `CODEOWNERS` file **should** automatically request review from the owners of the paths a pull request changes. | GitHub, GitLab and Gitea all read `CODEOWNERS`. | should-have |

## Issues

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-COLLAB-020 | Each repository **should** have an issue tracker, which administrators can turn off. | All four. | should-have |
| FR-COLLAB-021 | Issues and pull requests **must** share one number sequence per repository and **must** support labels, assignees, milestones and comments. | GitHub and Gitea share numbering between issues and PRs, so `#12` is unambiguous. | must-have |
| FR-COLLAB-022 | Text in comments, descriptions and commit messages **must** turn `#<n>`, `<owner>/<repo>#<n>`, `@<user>` and commit IDs into links. | All four. | must-have |
| FR-COLLAB-023 | A merged pull request **should** close the issues it references with closing keywords (`fixes #n`, `closes #n`, `resolves #n`). | All four. | should-have |
| FR-COLLAB-024 | Repositories **may** provide issue and PR templates from `.klotho/`, and **should** also read `.github/` and `.gitea/` templates. | All four support templates. Reading other forges' paths makes imported repositories work straight away. | nice-to-have |

## Releases

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-COLLAB-030 | Users with write access **should** be able to create a release from a tag, with notes and attached binary assets. | All four. Atropos deploys from releases. | should-have |

## Notifications

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-COLLAB-040 | Users **must** get in-app notifications for mentions, review requests, and activity on issues and PRs they participate in or watch. | All four. | must-have |
| FR-COLLAB-041 | Notifications **should** also be sent by email when SMTP is configured, and users **should** be able to choose which events they get emails for. | All four. | should-have |
| FR-COLLAB-042 | Users **must** be able to watch or unwatch repositories and unsubscribe from a single thread. | All four. | must-have |

## Conflicts with the current implementation

None: collaboration features haven't been started (commit `6b4895f`).
