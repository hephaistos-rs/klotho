# Storage requirements

**Scope:** where and how Klotho keeps its data. That means the data directory, the git data on disk, the metadata store, file storage for non-git data (local or S3), how repository identities map to paths, atomicity, durability, integrity, housekeeping, backup and quotas. See [ADR 0003](../decisions/0003-data-and-file-storage.md).

**Not in scope:** name syntax ([naming.md](naming.md)), repository lifecycle operations ([repositories.md](repositories.md)), and how fast storage has to be ([performance.md](performance.md)).

## Metadata and layout

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-STOR-001 | Klotho **must** keep a metadata store, separate from the git data, that holds at least: repositories (ID, owner, display name, key), owners, redirects and credentials. | GitLab, Gitea and Forgejo all use a database next to their bare repositories. Display names (FR-NAME-020) and redirects (FR-NAME-050) can't be derived from the filesystem. | must-have |
| FR-STOR-002 | The metadata store **should** default to embedded SQLite and **should** also support PostgreSQL. | Gitea and Forgejo ship with SQLite for small installs and support PostgreSQL for larger ones. A single-binary self-hosted install needs zero setup. | should-have |
| FR-STOR-003 | Each repository **must** have an immutable internal ID, assigned at creation and never reused. | All four identify repositories by ID internally. Redirects (FR-NAME-052) and ID-based paths (FR-STOR-004) need it. | must-have |
| FR-STOR-004 | A repository's location on disk **should** be derived from its immutable ID, e.g. `<root>/<h[0:2]>/<h[2:4]>/<h>.git` where `h` is a hash of the ID, not from its name. | GitLab hashed storage makes the layout immutable, spreads repositories evenly, and never puts user input in a path. Case-insensitive filesystems and Windows device names stop mattering. | should-have |
| FR-STOR-005 | Creating a repository **must** be atomic: when two requests race to create the same key, exactly one succeeds and the other gets "already exists". A half-initialised repository **must never** be visible to `open`, `list` or git clients. | Needed for correctness. GitLab #3229 shows the damage duplicate entries cause. A standard way to do this is to initialise in a temporary directory and rename it into place. | must-have |
| FR-STOR-006 | Renaming or transferring a repository **should not** move or copy its git data. | GitLab: a rename "costs only the database transaction". Gitea has to move directories, which is slow and can leave the database and disk out of step. | should-have |
| FR-STOR-007 | Any path built from a user-supplied value **must** be a valid, usable git repository path on every supported platform. On Windows this means it can't be a device name (`con`, `prn`, `aux`, `nul`, `com1`–`com9`, `lpt1`–`lpt9`, any case, with or without an extension). | Verified: Git for Windows refuses to use `con.git` although the OS creates it (see [forge-comparison.md § 5](../research/forge-comparison.md#5-how-names-map-to-paths-on-disk)). FR-STOR-004 satisfies this automatically. | must-have |
| FR-STOR-008 | Storage **must** behave the same on case-sensitive and case-insensitive filesystems. Two names that differ only in case must never map to two different paths. | Gitea lowercases paths for exactly this reason. Klotho is developed on Windows and deployed on Linux. | must-have |
| FR-STOR-009 | A repository's wiki (if implemented) **should** be stored as a sibling bare repository derived from the same ID, e.g. `<h>.wiki.git`. | GitLab, Gitea and GitHub all keep the wiki as a separate git repository next to the main one. | nice-to-have |
| FR-STOR-010 | The storage root **must** be configurable. It defaults to `<data_dir>/repositories` (FR-STOR-012). | Universal. | must-have |
| FR-STOR-012 | All local state **must** live under one configurable data directory by default: `repositories/`, the SQLite database and `files/` (FR-STOR-060). Each location **may** be overridden on its own. A relative path **must** be resolved against the config file's directory, not the current directory, and the resolved absolute paths **must** be logged at startup. | Gitea's `APP_DATA_PATH`. One folder makes installs, backups and container volumes simple. Resolving against the current directory means starting the server from another folder silently creates an empty instance. | must-have |
| FR-STOR-011 | Klotho **may** support several storage roots, choosing one per repository when it is created. | GitLab lets admins spread repositories over several storage locations to grow capacity. | nice-to-have |

## File storage (non-git data)

Git LFS objects, release assets, attachments, avatars, cached archives and backups: large files that Klotho stores and serves but never reads with git. See [ADR 0003](../decisions/0003-data-and-file-storage.md).

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-STOR-060 | All non-git files **must** be stored through one file storage interface, with a local directory (`<data_dir>/files/`) as the default backend. | One code path for uploads, downloads, deletion and backup, whatever the backend. | must-have |
| FR-STOR-061 | File storage **should** also support any S3-compatible object store (AWS S3, MinIO, Garage, Cloudflare R2, Backblaze B2), configured by bucket, region, endpoint URL, path-style addressing and credentials. | Gitea, Forgejo and GitLab all offer this. LFS and release assets are what fills disks, and object storage is cheaper and grows without resizing. | should-have |
| FR-STOR-062 | The metadata store **must** refer to files by a relative key (e.g. `lfs/ab/cd/<oid>`), never by a filesystem path or URL. | Moving between backends is then a plain copy with no database migration. | must-have |
| FR-STOR-063 | Downloads **must** be authorised by Klotho first. With an S3 backend, Klotho **may** then redirect to a presigned URL valid for at most 5 minutes instead of streaming the file itself. | Large downloads skip the Klotho server. A short expiry limits how long a URL works after access is revoked. | should-have |
| FR-STOR-064 | Git repositories **must** be stored on a local or block-device filesystem. They **must not** be stored in object storage or on a bucket mounted as a filesystem. | git needs atomic renames, lock files, random reads and `fsync`, which object storage doesn't provide. Every comparable forge keeps repositories on local disk. | must-have |

## Consistency

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-STOR-020 | Directories under the storage root that don't match a known repository, and known repositories whose directory is missing, **must** be reported to administrators (a log warning at startup plus an admin view or CLI command), not silently ignored. | Gitea surfaces unadopted repositories in its admin panel. Silently hiding data makes problems hard to find. | should-have |
| FR-STOR-021 | Metadata and git data changes that belong together, such as creating, deleting or transferring a repository, **must** leave the system consistent if the process crashes at any point. Either both are applied, or recovery on the next start completes or rolls back the operation. | Gitea's name-based moves can leave the database and disk disagreeing after a crash. | must-have |

## Durability and integrity

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| NFR-STOR-001 | Once a push has been reported as successful, its objects and ref updates **must** survive power loss. For example, git must run with `core.fsync` covering committed objects and refs. | Git only guarantees this with the right `core.fsync` settings. Losing data after telling the client the push succeeded is the worst failure a forge can have. | must-have |
| NFR-STOR-002 | A push interrupted at any point **must** leave refs either fully at their old values or fully at their new values. It must never leave a ref pointing to a missing object. | `git receive-pack` guarantees this when refs are updated through it. Klotho must not bypass it. | must-have |
| FR-STOR-030 | Klotho **should** run a scheduled integrity check (`git fsck` or equivalent) over all repositories and report failures to administrators. | GitLab (repository checks) and Gitea (cron `git fsck`). | should-have |
| FR-STOR-031 | Klotho **should** run repository housekeeping (repack, prune, commit-graph and bitmap generation) on a schedule and after a configurable number of pushes. | GitLab housekeeping and Gitea `git gc` cron. Stops fetches slowing down as repositories age. | should-have |

## Backup and restore

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-STOR-040 | Administrators **must** be able to take a consistent backup of all git data and metadata while the server is running. | GitLab `backup create` and Gitea `dump`. Self-hosters are their own disaster recovery. | must-have |
| FR-STOR-041 | Administrators **must** be able to restore a backup onto a fresh install of the same version, ending with an identical set of repositories, refs, owners and redirects. | A backup that can't be restored is worth nothing. GitLab and Gitea both document the restore path. | must-have |
| FR-STOR-042 | Backups **must** include the file storage when it's local. When it's S3, the admin **must** be able to choose between copying the files into the backup and relying on the bucket's own versioning or replication, and the backup **must** record which was chosen. | Otherwise restoring brings back database rows whose LFS objects or attachments are missing. | must-have |
| NFR-STOR-010 | The backup and restore procedure **should** be covered by an automated test that backs up, restores and compares all refs. | Makes FR-STOR-041 verifiable. | should-have |

## Quotas and deduplication

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-STOR-050 | Administrators **should** be able to set per-owner and per-repository disk quotas, and pushes that would exceed a quota **should** be rejected with a clear message. | Forgejo quotas and GitLab storage limits. Stops one tenant filling the disk. | should-have |
| FR-STOR-051 | Forks **may** share objects with their upstream through git alternates or an object pool. | GitLab `@pools` and GitHub fork networks. Saves disk space for popular repositories. It also means commits pushed to one fork can be reached through the others, which has to be considered before building it. | nice-to-have |

## Conflicts with the current implementation

Checked after Phase 1 (2026-10-01).

| Requirement | Current behaviour | Where |
|---|---|---|
| NFR-STOR-001 (durable pushes) | **Not ensured.** Pushes still go through `git receive-pack` with git's default `core.fsync`. The native engine (Phase 1b, ADR 0004) fsyncs packs and refs itself. | [git_http.rs](../../crates/server/src/git_http.rs) |

Met today:
- FR-STOR-001, 003 (a SQLite metadata store; repository IDs that are never reused, thanks to `AUTOINCREMENT`).
- FR-STOR-004, 006, 007, 008 (paths derived from a hash of the ID, so names never reach the filesystem: renames won't move data, and `con` or `Demo`/`demo` mean nothing to the filesystem).
- FR-STOR-005 (atomic create: the unique index decides races, the repository is initialised in `.tmp/` and renamed into place; 50 concurrent creates give one success and no leftovers, tested).
- FR-STOR-010, 012 (configurable storage root inside one `data_dir`).
- FR-STOR-020 (unadopted directories and missing repositories are reported at startup, by `klotho admin unadopted` and by `GET /api/v1/admin/unadopted`).
- FR-STOR-021 for create and adopt: a crash before the commit leaves a directory for an ID with no row; the next create of that ID moves it to `.orphaned/` instead of failing, and `.tmp/` and `.trash/` are emptied at startup.
