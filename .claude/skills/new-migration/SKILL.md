---
name: new-migration
description: Add a database schema change to Klotho - a new sqlx migration, the queries that use it, and the regenerated .sqlx offline cache. Use whenever a change needs a new table, column or index.
argument-hint: "<short_name, e.g. ssh_keys>"
---

# Adding a migration

SQLite migrations live in `crates/core/migrations/sqlite/` and run automatically at startup (NFR-OPS-031). sqlx checksums every applied migration, so **a committed migration is never edited**; a hook blocks it. Klotho is unreleased, but developers' databases (`testRepos/`) have already applied the existing ones.

1. **Create the file** `crates/core/migrations/sqlite/<YYYYMMDDHHMMSS>_<name>.sql`, with a timestamp later than every existing file (list them first). Use today's date; if a migration from today already exists, increment the time part.
2. **Write the SQL** for SQLite:
   - Start with a comment naming the phase and requirement IDs, like the existing files.
   - Follow the conventions at the top of the first migration: `STRICT` tables; times are `INTEGER` seconds since the Unix epoch (UTC), converted with `secs()` and `time()` in `meta/sqlite.rs`; booleans are `INTEGER NOT NULL DEFAULT 0 CHECK (x IN (0, 1))`; foreign keys spell out `ON DELETE` and are indexed; secret hashes are checked to be 32 bytes. Use `WITHOUT ROWID` for tables keyed by a hash or a composite key.
   - Store only hashes of secrets (SHA-256 for tokens, Argon2id for passwords). Secrets that must be read back are encrypted (NFR-SEC-042).
   - Case-insensitive unique names use a separate `*_key` column holding the lowercased value.
   - Add indexes for every foreign key you query by.
3. **Query it only in the metadata store**, never in a service: add a method to `MetaStore` (or `WriteTx`, for writes that must happen together) in `crates/core/src/meta/mod.rs`, in domain terms (parsed names, `jiff::Timestamp`, domain errors for unique conflicts), and implement it in `meta/sqlite.rs` with `sqlx::query!` / `query_as!` / `query_scalar!` (checked at compile time), never unchecked `sqlx::query`.
4. **Regenerate the offline cache:** `cargo xtask sqlx-prepare`. This creates a fresh database, runs every migration and rewrites `.sqlx/`.
5. **Stage `.sqlx/` completely**, including deleted query files: `git add -A .sqlx crates/core/migrations crates/core/src/meta`.
6. **Run the checks** (`cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`). Tests build their database from the migrations, so a broken migration fails every test.
7. If the change affects `docs/design/auth-flows.md` "Data model" or another design doc, update it in the same change.
