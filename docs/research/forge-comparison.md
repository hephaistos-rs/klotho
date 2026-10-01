# Forge comparison: repository naming

How GitHub, GitLab, Gitea and Forgejo name, look up, store and redirect repositories, and what Klotho should take from each. Researched September 2026 from official documentation and, for Gitea and Forgejo, from their source code. Sources are listed at the end.

Forgejo is a hard fork of Gitea, so on naming they are almost identical. Where this document says "Gitea/Forgejo" the behaviour is shared; differences are called out.

## Summary

- **All four look names up case-insensitively and keep them unique case-insensitively, but all four display a name with the casing its owner typed.** Klotho is the only one of the five that throws the casing away.
- **They split on how names map to disk.** GitLab stores repositories under a hash of an immutable project ID, so renames never touch the filesystem. Gitea/Forgejo store them under the lowercased owner and repository name, so a rename is a directory move. GitHub's layout is internal.
- **All four accept clone URLs with or without `.git`.** For a *new* name ending in `.git`, GitHub strips the suffix and tells you. GitLab, Gitea and Forgejo reject the name.
- **All four redirect old names after a rename,** until someone claims the old name. Only the redirect's scope differs: web, git and API traffic are redirected, but things like GitHub Actions `uses:` references and Pages sites aren't.
- **Reserved names come from URL routing.** Each platform reserves names that would collide with its own routes (`api`, `login`, `*.atom`, and so on). GitLab and Gitea also move new routes under a `/-/` prefix so the reserved list doesn't keep growing. Forgejo explicitly rejects that approach.

## 1. Case sensitivity

| | GitHub | GitLab | Gitea | Forgejo |
|---|---|---|---|---|
| Lookup | Case-insensitive | Case-insensitive; the web UI redirects to the canonical casing | Case-insensitive (`lower_name` column) | Same as Gitea |
| Uniqueness | Case-insensitive | Case-insensitive (`UNIQUE (namespace_id, lower(path))`) | Case-insensitive (`UNIQUE` on `lower_name`) | Same as Gitea |
| Display | Casing as entered | Casing as entered | Casing as entered (`name` column) | Same as Gitea |
| On disk | Not name-based (internal) | Not name-based (hash of the ID) | Lowercased: `<root>/<lower(owner)>/<lower(repo)>.git` | Same as Gitea |

**How they get there.** Gitea/Forgejo keep two columns, `Name` (display) and `LowerName` (unique index, used for every query). The disk path is built with `strings.ToLower`, so casing is irrelevant on every filesystem. GitLab enforces case-insensitive uniqueness with a `lower(path)` index on its `routes` table.

**Known pain points.**

- *Gitea #5489:* a user created `AppData` and found `appdata.git` on disk. Scripts working directly on the storage root and `git clone --mirror` backups can't recover the original casing from the filesystem alone. Lowercasing on disk is safe, but the canonical name has to live somewhere else, in the database.
- *GitLab #44029:* renaming a group from `ABC` to `abc` created a redirect `ABC → abc`. That redirect was pointless, because lookups were already case-insensitive, and it later blocked settings changes with "Route path has been taken before". **A rename that only changes case must not create a redirect.**
- *GitLab #3229 / #18647:* before uniqueness was enforced in the database, `FooBar` and `foobar` could both exist and redirects sent users to the wrong one. Case-insensitive uniqueness has to be enforced where the names are stored, not only in application code.

## 2. Allowed characters, length and reserved names

### Repository names

| | GitHub | GitLab (path/slug) | Gitea / Forgejo |
|---|---|---|---|
| Characters | ASCII letters, digits, `.`, `-`, `_` | ASCII letters, digits, `.`, `-`, `_` | `^[-.\w]+$`, i.e. ASCII letters, digits, `-`, `.`, `_` |
| Invalid input | Converted to `-`, with a "will be created as …" notice | Rejected | Rejected |
| Start / end | No documented rule (`.github` is a valid, special repo) | Must start **and** end with a letter or digit | No rule beyond the reserved names |
| Consecutive specials | Allowed | **Not allowed** (`a..b`, `a-_b`) | `..` rejected in current Gitea |
| Max length | 100 | 255 | 100 |
| Reserved exact names | `.`, `..` | `-`, `badges`, `blame`, `blob`, `builds`, `commits`, `create`, `create_dir`, `edit`, `files`, `find_file`, `new`, `preview`, `raw`, `refs`, `tree`, `update`, `wikis`, … | `.`, `..`, `-` |
| Reserved suffixes | `.wiki` (rejected); `.git` (stripped) | `.git`, `.atom` | `.git`, `.wiki`, `.rss`, `.atom` |
| Display name separate from slug | No (description only) | **Yes.** Project *name* allows Unicode, emoji, spaces and `+`; the *path* is the slug | No |

### Owner (user / organisation) names

| | GitHub | GitLab | Gitea | Forgejo |
|---|---|---|---|---|
| Characters | ASCII alphanumerics and single hyphens; no leading or trailing hyphen | Letters, digits, `_`, `.`, `-`; start and end alphanumeric; no consecutive specials | Alphanumerics, `-`, `_`, `.` | Same as Gitea |
| Max length | 39 | 255 | 40 | 40 |
| Shared namespace for users and orgs | Yes | Yes (users and groups) | Yes | Yes |
| Reserved | Route names (not published as a list) | Top level: `-`, `.well-known`, `admin`, `api`, `assets`, `dashboard`, `explore`, `groups`, `help`, `login`, `oauth`, `profile`, `projects`, `public`, `search`, `snippets`, `uploads`, `users`, `v2`, static files, … | `.`, `..`, `.well-known`, `api`, `metrics`, `v2`, `assets`, `attachments`, `avatar(s)`, `repo-avatars`, `captcha`, `login`, `org`, `repo`, `user`, `explore`, `issues`, `pulls`, `milestones`, `notifications`, `favicon.ico`, `manifest.json`, `robots.txt`, `sitemap.xml`, `ssh_info`, `swagger.v1.json`, `ghost`, `gitea-actions` | Gitea's list plus `-`, `admin`, `report_abuse`, `forgejo-actions`, `actor` |
| Reserved patterns | — | Names starting with a route name followed by `.` (e.g. `admin.`) | `*.keys`, `*.gpg`, `*.rss`, `*.atom`, `*.png` | Same as Gitea |

**Why the reserved suffixes exist.** Each one is a route: `/<user>.keys` serves SSH keys, `.gpg` GPG keys, `.rss`/`.atom` feeds, `.png` avatars. `.wiki` is reserved because a repository's wiki is stored next to it as `<repo>.wiki.git`. A repository called `foo.wiki` would take the wiki's place on disk (Gitea) or its URL (GitHub).

**Keeping the reserved list stable.** GitLab puts project sub-pages under `/<namespace>/<project>/-/…` (e.g. `/-/issues`). Gitea has started doing the same for instance-level routes (`/-/admin`), which let it un-reserve names like `admin`. Forgejo declined to follow (issue #8030) and reserves the names instead, accepting that adding one later is a breaking change.

## 3. The `.git` suffix

| | GitHub | GitLab | Gitea / Forgejo |
|---|---|---|---|
| Clone URL with `.git` | Works | Works | Works |
| Clone URL without `.git` | Works | Works | Works |
| Clone URL shown in the UI | Ends in `.git` | Ends in `.git` | Ends in `.git` |
| Creating a repo named `foo.git` | Suffix stripped; created as `foo` with a notice | Rejected | Rejected (`*.git` is reserved) |

All four treat `.git` as part of the URL, not part of the name. The only disagreement is whether a new name ending in `.git` is silently fixed or refused. Refusing is more predictable: the name you asked for is either the name you get or an error.

## 4. Renames and redirects

| | GitHub | GitLab | Gitea / Forgejo |
|---|---|---|---|
| Web redirects | Yes | Yes | Yes |
| Git clone/fetch/push via the old URL | Yes | Yes, and git prints a warning to update the remote | Yes |
| API via the old name | Yes (301) | Yes | Yes |
| Owner rename redirects that owner's repos | Yes | Yes (users and groups) | Yes (`user_redirect` table) |
| Owner rename redirects the profile page | **No** (404) | Yes | Yes |
| Transfer to another owner redirects | Yes | Yes | Yes |
| Redirect lifetime | Until the old name is reused | "As long as the original path is not claimed" | Until the old name is reused |
| Not redirected | Actions `uses:` references, Pages sites | CI `include:`, container image paths, CODEOWNERS, encoded-path API calls | — |
| Rename cost on disk | None (internal storage) | **None** (hashed storage) | Directory move |

**How they're stored.** Gitea/Forgejo keep a `repo_redirect` table of `(owner_id, lower_name) → repo_id`. It's consulted only when a normal lookup finds nothing and is deleted when the name is claimed again. Because it points at an **ID**, a chain of renames (`a → b → c`) keeps working. GitLab keeps a `redirect_routes` table with the same semantics.

**Name reuse is a supply-chain risk.** Once the old name is free, someone else can register it and silently take over every URL that still points there ("repo-jacking"). GitHub mitigates this by permanently *retiring* the `OWNER/REPO` pair when a popular repository's owner is renamed or deleted, or when the repository is transferred. "Popular" means a Marketplace action, or more than 100 clones or Actions uses in the preceding week. A retired pair can never be claimed again. None of the others do this.

## 5. How names map to paths on disk

| | Layout | Consequences |
|---|---|---|
| **GitHub** | Internal and not publicly documented. Forks share an object "network". | Renames are metadata-only. Objects are shared across a fork network, which is why commits pushed to a fork can be reached through the parent's URL. |
| **GitLab** | `@hashed/<h[0:2]>/<h[2:4]>/<h>.git`, where `h = SHA256(project_id)`. Wiki: `<h>.wiki.git`. Fork deduplication: `@pools/…`. | "Makes the folder structure immutable and eliminates the need to synchronize state from URLs to disk structure." Renames and transfers cost one database transaction. No user input reaches the filesystem. Repositories are spread evenly across directories. The cost is that the disk layout is unreadable without the database. |
| **Gitea / Forgejo** | `<root>/<lower(owner)>/<lower(repo)>.git`; wiki `<repo>.wiki.git` | Readable on disk. Works on case-insensitive filesystems. Renames and transfers must move directories, and must be kept in step with the database. User-chosen names become path components, so every name rule is also a filesystem-safety rule. |

**Platform note.** None of the four has to worry about Windows, because all of them are deployed on Linux. Klotho is being developed on Windows, and there a name-based layout has a gap no forge covers. Windows 11 lets you create a directory called `con.git` (or `nul.git`, `aux.git`, `com1.git`, …), but **Git for Windows refuses to operate on it**. We checked on this machine with Git 2.55.0.windows.5: Klotho created a repository named `con`, and `git clone` of it failed with HTTP 500 because `git upload-pack` reported "does not appear to be a git repository". An ID-based layout avoids this completely; a name-based one needs a device-name blocklist.

## Where they agree

1. Case-insensitive lookup and uniqueness, with display casing preserved.
2. ASCII letters, digits, `-`, `_` and `.` as the base character set for URL names.
3. `.git` optional in clone URLs, and always shown in the canonical clone URL.
4. Redirects after renames, owner renames and transfers, for web, git and API traffic. The redirect lives until the name is claimed again.
5. One shared namespace for users and organisations, with names reserved for top-level routes.
6. The wiki is a separate `<repo>.wiki.git`, so `.wiki` names are reserved.

## Where they differ

| Topic | Options seen |
|---|---|
| Storage layout | Keyed by ID (GitLab) vs. lowercased name (Gitea/Forgejo) |
| `.git` on a new name | Strip with a notice (GitHub) vs. reject (GitLab, Gitea, Forgejo) |
| Character rules | Permissive (Gitea) → sanitising (GitHub) → strict start/end and no consecutive specials (GitLab) |
| Length | 100 (GitHub, Gitea, Forgejo) vs. 255 (GitLab) |
| Display name | Separate human-friendly name and slug (GitLab) vs. one name |
| Growing the route space | `/-/` prefix (GitLab, Gitea) vs. extending the reserved list (Forgejo) |
| Profile redirect after an owner rename | No (GitHub) vs. yes (GitLab, Gitea) |
| Protecting abandoned names | Namespace retirement for popular repos (GitHub) vs. none |

## What Klotho should borrow

| # | Idea | From | Why |
|---|---|---|---|
| 1 | Keep two forms of every name: the **display** form (as typed) and a **key** form (ASCII-lowercased, unique) | All four | Gives case-insensitive behaviour without losing what the user typed. Klotho currently keeps only the key form. |
| 2 | Put repositories on disk under an **immutable ID**, not a name | GitLab | Makes renames and transfers free and makes case-sensitivity and Windows device names irrelevant. User input never becomes a path. |
| 3 | Keep a **redirect table** from the old key to the repo ID. Consult it only on a miss, delete the entry when the name is claimed, and create none for case-only renames | Gitea/Forgejo, GitLab (and #44029) | Chains of renames keep working and avoid GitLab's case-rename bug. |
| 4 | **Reject** new names ending in `.git`, `.wiki`, `.atom`, `.rss`; strip `.git` only when *resolving* URLs | GitLab, Gitea/Forgejo | The name you ask for is either the name you get or an error. It also leaves room for wikis and feeds. |
| 5 | Put all non-owner routes under **one reserved prefix** (`/-/`) from day one | GitLab, Gitea | Keeps the reserved-name list small and fixed, so future features never break existing names. Forgejo shows the cost of retrofitting. |
| 6 | Forbid a leading `-` and the names `.`/`..`; forbid `..` anywhere | Gitea, GitLab | Blocks path and argument confusion. The stricter GitLab rules (start and end alphanumeric, no consecutive specials) would make some valid GitHub names unimportable, so they aren't worth adopting. |
| 7 | Cap repo names at 100 and owner names at 39 characters | GitHub, Gitea | Any GitHub name then fits, which makes imports lossless. |
| 8 | Optionally separate a free-form **display title** from the URL name | GitLab | Lets people use Unicode and spaces without affecting URLs. Low priority. |
| 9 | **Retire** owner/name pairs of busy repositories instead of releasing them | GitHub | Guards against repo-jacking on multi-tenant instances. Low priority for single-team installs. |
| 10 | Let git print a redirect warning by answering the old URL with an HTTP redirect, instead of serving it silently | GitLab | Git already prints "warning: redirecting to …", so users learn to update their remote at no extra cost. |

## Sources

- GitHub: [Renaming a repository](https://docs.github.com/en/repositories/creating-and-managing-repositories/renaming-a-repository); [Username changes](https://docs.github.com/en/account-and-profile/concepts/username-changes); [Transferring a repository (namespace retirement)](https://docs.github.com/en/repositories/creating-and-managing-repositories/transferring-a-repository); [Deleting an organization (namespace retirement)](https://docs.github.com/en/organizations/managing-organization-settings/deleting-an-organization-account); [github/docs #44518, repository name limits](https://github.com/github/docs/issues/44518)
- GitLab: [Reserved project and group names](https://docs.gitlab.com/user/reserved_names/); [Repository storage paths (hashed storage)](https://docs.gitlab.com/administration/repository_storage_paths/); [Repository path changes](https://docs.gitlab.com/user/project/repository/); [#44029, case-only group rename](https://gitlab.com/gitlab-org/gitlab-foss/-/issues/44029); [#3229, case handling](https://gitlab.com/gitlab-org/gitlab-foss/-/issues/3229); [#18647, case-insensitive paths in the DB](https://gitlab.com/gitlab-org/gitlab-foss/-/issues/18647)
- Gitea: [`models/repo/repo.go`](https://github.com/go-gitea/gitea/blob/main/models/repo/repo.go); [`models/user/user.go`](https://github.com/go-gitea/gitea/blob/main/models/user/user.go); [`models/repo` package docs (`RepoPath`, `Redirect`)](https://pkg.go.dev/gitea.dev/models/repo); [#5489, lowercase storage](https://github.com/go-gitea/gitea/issues/5489); [#6280, custom reserved names](https://github.com/go-gitea/gitea/issues/6280)
- Forgejo: [`models/repo/repo.go`](https://codeberg.org/forgejo/forgejo/src/branch/forgejo/models/repo/repo.go); [`models/user/user.go`](https://codeberg.org/forgejo/forgejo/src/branch/forgejo/models/user/user.go); [#8030, `/-/` routes rejected](https://codeberg.org/forgejo/forgejo/issues/8030)
- Windows device-name behaviour: tested locally, 2026-09-29, Windows 11 Pro 10.0.26200, Git 2.55.0.windows.5.
