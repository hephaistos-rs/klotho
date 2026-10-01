# ADR 0003: Data directory, and S3-compatible storage for non-git files

- **Status:** accepted
- **Date:** 2026-10-01

## Context

Klotho stores three kinds of data:

1. **Git repositories.** They're read with gix and served by running `git upload-pack` / `git receive-pack` ([ADR 0001](0001-stack.md)).
2. **Metadata.** SQLite by default, PostgreSQL optionally (FR-STOR-002).
3. **Everything else: large, write-once files that Klotho only stores and serves.** Git LFS objects (FR-GIT-030), release assets (FR-COLLAB-030), issue and pull request attachments, avatars, cached archive downloads (FR-GIT-012) and backups (FR-STOR-040).

Self-hosters increasingly want object storage (AWS S3, MinIO, Garage, Cloudflare R2, Backblaze B2) for the third kind, because it's cheap, grows without resizing disks, and is often backed up already. LFS especially can grow far larger than the repositories themselves.

Today the code has a single `KLOTHO_REPOS` path, defaulting to a folder relative to the current directory.

## Decision

### Git repositories stay on a local filesystem

Repositories live in a directory on a local or block-device filesystem (FR-STOR-064), never in object storage or on a bucket mounted as a filesystem (s3fs, mountpoint-s3).

- `git` and gix need POSIX semantics: atomic rename, file locks (`*.lock` files for ref updates), random reads into packfiles, and `fsync`. Object storage offers none of these, and FUSE adapters only imitate them, with corruption or terrible latency under concurrent pushes.
- Every comparable forge does the same: Gitea and Forgejo (local disk), GitLab (Gitaly, local disk per node) and GitHub (Spokes, local disk replicated across nodes). Growing repository storage means more storage roots (FR-STOR-011), not object storage.

### Everything else goes through one file store, local by default, S3 optionally

- **One interface, `FileStore`, in `klotho-core`**, for every non-git file. We call these *files*, not *objects* or *blobs*, so they aren't confused with git's objects and blobs.
- **Built on the [`object_store`](https://crates.io/crates/object_store) crate** (Apache Arrow, 0.14). It provides one async trait over the local filesystem, S3 and S3-compatible stores, GCS and Azure, with streaming uploads and downloads, multipart upload and presigned URLs. Klotho wraps it in a thin `FileStore` so the rest of the code never sees which backend is in use.
- **Backends:** `local` (the default: `<data_dir>/files/`) and `s3` (any S3-compatible endpoint: bucket, region, endpoint URL, path-style option, credentials). Support for GCS or Azure can come later through the same crate.
- **The database stores keys, never paths or URLs** (FR-STOR-062). A key is a relative, content- or ID-derived name such as `lfs/ab/cd/abcdef…` or `attachments/<uuid>`. Moving from local to S3 is then a plain copy of `<data_dir>/files/` into the bucket, with no database change.
- **Downloads go through one function** (FR-STOR-063). After `authorize()`, it either streams the file through Klotho (`local`) or answers `302` to a presigned URL valid for a few minutes (`s3`, if enabled). LFS clients and browsers follow the redirect, so large downloads don't pass through Klotho.
- **S3 credentials are secrets.** They come from the config file or environment variables, never from the database, and are kept out of logs (NFR-OPS-023).

### One data directory

All local state lives under a single configurable `data_dir` (FR-STOR-012):

```text
<data_dir>/
  repositories/   # bare git repositories, ID-hashed paths (FR-STOR-004)
  klotho.db       # SQLite metadata store
  files/          # the local FileStore backend
```

- Each location can be overridden on its own (for example, `repositories` on a separate disk), but nobody has to set more than `data_dir`.
- **Relative paths resolve against the config file's directory**, not the current directory, and the absolute path is logged at startup. Otherwise running the server from a different folder silently starts an empty instance.
- The default is `./data`, as Gitea does. Development uses a `klotho.dev.toml` in the repository (or, until the config file exists in Phase 0, `KLOTHO_REPOS` in `.cargo/config.toml`), so test paths never end up as defaults in the code.

## Alternatives considered

| Option | Why not |
|---|---|
| **Repositories in S3 as well** | Covered above: git needs a POSIX filesystem. Reimplementing git storage on top of object storage (as some research systems do) is a large project outside Klotho's scope. |
| **Hand-written S3 client, or `aws-sdk-s3`** | `aws-sdk-s3` is large, slow to compile and AWS-flavoured, and S3-compatible servers each have their quirks. `object_store` already handles them, and gives us the local backend through the same trait. |
| **Files on local disk only** | Simpler, but LFS and release assets are exactly what makes disks fill up. Adding S3 later would be cheap only if keys are used from the start, so we decide the interface now and build the S3 backend when it's needed. |
| **Store small files in the database** | Avatars would fit, but splitting files by size means two code paths, and it bloats backups of the database. |

## Consequences

**Good:**
- Admins can put the bulky data (LFS, releases) on cheap object storage, while repositories keep the local disk they need.
- One data folder makes installs, backups and Docker volumes simple.
- Switching backends is a copy, not a migration.

**Bad:**
- One more crate (`object_store`) and one more config section.
- Backups have to cover two places when S3 is used. `klotho admin backup` copies repositories and the database. Files in S3 are either copied too, or left to the bucket's own versioning and replication, as the admin chooses (FR-STOR-042).
- Presigned URLs make access decisions final for a few minutes: a user who loses access can still finish a download they've just started.

**Requirement changes made with this decision:**
- **Added:** FR-STOR-012, FR-STOR-042, FR-STOR-060 to FR-STOR-064.

**Revisit if:** Klotho needs to run repositories across several servers (then the question is replication, as with Gitaly or Spokes, not object storage).
