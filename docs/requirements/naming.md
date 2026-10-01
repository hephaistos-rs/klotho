# Naming requirements

**Scope:** the syntax and identity of repository and owner (user or organisation) names. That covers allowed characters, case handling, reserved names, the `.git` suffix in URLs, and how old names resolve after renames and transfers.

**Not in scope:** the rename and transfer *operations* themselves ([repositories.md](repositories.md)) and how names map to disk paths ([storage.md](storage.md)).

Background for every rationale below: [forge-comparison.md](../research/forge-comparison.md).

**Terms**

- **Display name:** the name as its owner typed it, e.g. `MyRepo`.
- **Key:** the display name with ASCII letters lowercased, e.g. `myrepo`. Used for uniqueness and lookup.
- **Full name:** `<owner>/<repo>`.

## Syntax

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-NAME-001 | Repository and owner names **must** contain only ASCII letters, digits, `-`, `_` and `.`. | The character set shared by GitHub, GitLab slugs and Gitea/Forgejo. It is safe in URLs, shells and filesystems. | must-have |
| FR-NAME-002 | Repository names **must** be 1–100 characters long. | GitHub, Gitea and Forgejo all cap at 100, so any GitHub name can be imported. | must-have |
| FR-NAME-003 | Owner names **must** be 1–39 characters long. | GitHub's limit (39) is the tightest of the four, so any GitHub owner can be imported. | must-have |
| FR-NAME-004 | Names **must not** start with `-`. | Stops a name from being read as a command-line option by `git` or admin scripts. All four forbid it. | must-have |
| FR-NAME-005 | Names **must not** be `.` or `..` and **should not** contain `..` anywhere. | Gitea/Forgejo reserve `.`/`..` and current Gitea rejects `..`. Removes any chance of path traversal. | must-have |
| FR-NAME-006 | Repository names **may** start with `.`, e.g. `.github` or `.profile`, as long as they satisfy FR-NAME-005. | GitHub (`.github`) and Gitea/Forgejo (`.profile`) use dot-names for special repositories, so importing them needs this. | nice-to-have |
| FR-NAME-007 | Names containing characters outside FR-NAME-001 **must** be rejected with an error naming the offending character. The UI **may** offer a sanitised suggestion, such as replacing the character with `-`. | GitHub converts silently with a notice. GitLab and Gitea reject. Rejecting keeps the API predictable, and the suggestion keeps GitHub's convenience. | should-have |
| FR-NAME-008 | The same validation **must** apply wherever a name enters the system: create, import, rename, transfer and URL lookup. | A name that can be created but not looked up, or the reverse, is a bug by definition. All four validate in one model layer. | must-have |
| FR-NAME-009 | A repository **may** have an optional free-form display title (Unicode, spaces, up to 255 characters) that is shown in the UI but never used in URLs or on disk. | GitLab separates project *name* from *path*. People get readable titles without making URLs harder. | nice-to-have |

## Namespacing

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-NAME-010 | Every repository **must** belong to exactly one owner and be addressed by its full name `<owner>/<repo>` in URLs, the API and git remotes. | Universal across all four. It is needed for multi-user hosting and for importing from other forges. | must-have |
| FR-NAME-011 | Users and organisations **must** share one owner namespace: no user and organisation may have the same key. | All four do this, because `/<owner>` has to resolve to exactly one account. | must-have |
| FR-NAME-012 | Repository names **must** be unique per owner. Different owners **may** have repositories with the same name. | Universal. | must-have |

## Case

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-NAME-020 | The display name **must** be stored and returned exactly as entered: in API responses, the UI and the clone URLs shown to users. | All four keep the casing the user typed. Losing it breaks round-trips with other forges (Gitea #5489). | must-have |
| FR-NAME-021 | Name lookup **must** be case-insensitive. Only ASCII letters are folded, as `A–Z` → `a–z`. | All four resolve `Demo`, `DEMO` and `demo` to one repository. ASCII-only folding avoids Unicode case rules, which depend on locale. | must-have |
| FR-NAME-022 | Uniqueness **must** be enforced on the key, so creating `Demo` when `demo` exists fails with a conflict. The rule **must** be enforced by the store itself, not only by a check made before the write. | GitLab (`lower(path)` index), Gitea (`UNIQUE lower_name`). GitLab #3229 shows what happens when only the application code checks. | must-have |
| FR-NAME-023 | A web page request using non-canonical casing **should** redirect (301) to the canonical casing. API and git requests **should** be served directly. | GitLab redirects to the canonical case, so URLs people share look consistent. git and API clients don't benefit from a redirect. | should-have |
| FR-NAME-024 | Changing only the case of a name, e.g. `demo` → `Demo`, **must** be allowed and **must not** create a redirect entry. | GitLab #44029: case-only renames created pointless redirects that later blocked settings changes. | should-have |

## Reserved names

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-NAME-030 | Creating or renaming to a repository name whose key ends in `.git` **must** be rejected, not silently stripped. | GitLab, Gitea and Forgejo reject it. GitHub strips it but tells the user. Rejecting means the name you get is the name you asked for. | should-have |
| FR-NAME-031 | Repository names whose key ends in `.wiki`, `.atom` or `.rss` **must** be rejected. | Gitea/Forgejo reserve all three, GitHub `.wiki`, GitLab `.atom`. This keeps room for wiki repos and feeds without breaking existing names later. | should-have |
| FR-NAME-032 | Owner names **must not** use any key that is a top-level route. This covers at least `-`, `.well-known`, `api`, `admin`, `assets`, `login`, `logout`, `explore`, `favicon.ico`, `robots.txt`, `sitemap.xml` and `_topcoat` (Topcoat's runtime and asset routes), plus any path the server itself serves at the top level. | GitLab, Gitea and Forgejo all publish such lists. Without one, a user named `api` would hide or break the API. | must-have |
| FR-NAME-033 | Owner names **must not** end in `.keys`, `.gpg`, `.rss`, `.atom` or `.png`. | Gitea/Forgejo reserve these for key, feed and avatar routes (`/<user>.keys`, …). Reserving them before those routes exist avoids a breaking change later. | should-have |
| FR-NAME-034 | Every route that is not an owner or repository page **must** live under the reserved `/-/` prefix, e.g. `/-/admin` or `/<owner>/<repo>/-/issues`. The exceptions are those in FR-NAME-032 and the git smart-HTTP paths. | GitLab and Gitea use `/-/` so new features never need new reserved names. Forgejo #8030 shows what it costs to add this later. | should-have |
| FR-NAME-035 | Administrators **may** configure extra reserved names and glob patterns for owners and repositories. | A feature request against Gitea (#6280). Useful for org-specific names such as `security`. | nice-to-have |
| FR-NAME-036 | The server **must not** release owner/repository key pairs that an administrator has marked as retired, and **may** retire them automatically when a popular repository is renamed, transferred or deleted. | GitHub retires popular `OWNER/REPO` pairs to prevent repo-jacking. | nice-to-have |

## `.git` suffix in URLs

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-NAME-040 | Git transport URLs **must** resolve both with and without a trailing `.git`: `/<owner>/<repo>` and `/<owner>/<repo>.git` are the same repository. | All four. | must-have |
| FR-NAME-041 | Lookup **must** strip at most one trailing `.git` (ASCII case-insensitive), and only when resolving a URL, never when validating a new name. | Makes FR-NAME-030 and FR-NAME-040 agree: `foo.git.git` never resolves, and `foo.git` never becomes a valid *new* name. | must-have |
| FR-NAME-042 | The canonical clone URL shown to users and returned by the API **must** use the display name and end in `.git`. | All four show `.git` URLs. They work with every git client and are recognisable as remotes. | should-have |

## Renames, transfers and redirects

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-NAME-050 | After a repository is renamed, requests to the old full name **must** keep working for web pages (301), the API (301) and git clone/fetch/push. | GitHub, GitLab and Gitea all redirect all three. | must-have |
| FR-NAME-051 | Git requests to an old name **should** be answered with an HTTP redirect on `info/refs`, not served in place, so the client prints its "redirecting to" warning. | GitLab warns users to update their remote. `git` already follows initial redirects and prints a warning, so this costs nothing. | should-have |
| FR-NAME-052 | Redirects **must** resolve to the repository's identity, not to a name, so that after `a → b → c`, both `a` and `b` resolve to `c`. | Gitea's `repo_redirect` stores the target repo ID. Chains of renames keep working. | must-have |
| FR-NAME-053 | Creating a repository, or renaming one, to a key held by a redirect **must** succeed and delete that redirect. A real repository always wins over a redirect. | Universal ("until the old name is claimed"). | must-have |
| FR-NAME-054 | Renaming an owner **must** redirect all of that owner's repositories and **should** redirect the owner's profile page. | GitLab and Gitea redirect both. GitHub redirects repositories but not profiles, which confuses people. | should-have |
| FR-NAME-055 | Transferring a repository to another owner **must** create a redirect from the old full name, as a rename does. | GitHub, GitLab, Gitea. | should-have |
| FR-NAME-056 | Repository administrators **may** list and delete their repository's redirects. | Gives owners a way to release an old name on purpose. Gitea exposes redirects in its data model but not in the UI; adding the UI is our own idea. | nice-to-have |

## Conflicts with the current implementation

Checked after Phase 1 (2026-10-01): no conflicts. The rules live in [names.rs](../../crates/core/src/names.rs).

Met today: FR-NAME-001 to 008 (syntax, lengths, leading `-` and `.`, `..`, errors naming the offending character, one validator for every entry point), FR-NAME-010 to 012 (owner-scoped names in one owner namespace, unique per owner), FR-NAME-020 to 022 (display form kept, case-insensitive lookup, uniqueness enforced by a unique index), FR-NAME-030 to 033 (reserved suffixes and owner names), FR-NAME-034 (server routes live under `/-/`, apart from `/api` and git transport), FR-NAME-040 and 041 (`.git` optional in URLs, stripped once, only on lookup).

Not built yet, by plan: FR-NAME-023, 024 and 050 to 056 (canonical-case redirects, renames and redirects; Phase 6).
