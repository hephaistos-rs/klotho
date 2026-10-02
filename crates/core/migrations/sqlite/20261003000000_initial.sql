-- Klotho's metadata: owners, accounts and repositories (Phases 1 and 2;
-- docs/design/auth-flows.md, "Data model").
--
-- Conventions, for this file and every migration after it:
-- - Tables are STRICT, so a value of the wrong type is an error instead of
--   being stored as whatever SQLite converts it to.
-- - Times are INTEGER seconds since the Unix epoch, in UTC. They compare as
--   numbers, so expiry is checked in the WHERE clause. The API shows them as
--   RFC 3339 (FR-API-011).
-- - Booleans are INTEGER 0 or 1, checked.
-- - Case-insensitive unique names have a separate `*_key` column holding the
--   lowercased value. Uniqueness is enforced here, never by a check before the
--   insert (FR-NAME-022).
-- - Secrets are stored only as SHA-256 hashes (32 bytes) or Argon2id PHC strings.
-- - Every foreign key spells out ON DELETE, and every foreign key column is
--   indexed, for lookups and for the cascade.
-- - AUTOINCREMENT where an ID must never be reused, even after the newest row is
--   deleted (FR-STOR-003): repository paths and API IDs are derived from them.

-- Users and organisations share one namespace (FR-NAME-011). Organisations
-- arrive in Phase 7.
CREATE TABLE owners (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    kind       TEXT    NOT NULL CHECK (kind IN ('user', 'org')),
    name       TEXT    NOT NULL,          -- display form, as typed (FR-NAME-020)
    name_key   TEXT    NOT NULL UNIQUE,   -- ASCII-lowercased (FR-NAME-021)
    created_at INTEGER NOT NULL
) STRICT;

-- An account. The row shares its ID with its owner row.
CREATE TABLE users (
    id            INTEGER PRIMARY KEY REFERENCES owners (id) ON DELETE CASCADE,
    password_hash TEXT,                   -- NULL: can't sign in with a password
    is_admin      INTEGER NOT NULL DEFAULT 0 CHECK (is_admin IN (0, 1)),
    suspended_at  INTEGER                 -- NULL: active
) STRICT;

CREATE TABLE emails (
    id          INTEGER PRIMARY KEY,
    user_id     INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    address     TEXT    NOT NULL,          -- as typed
    address_key TEXT    NOT NULL UNIQUE,   -- lowercased
    is_primary  INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0, 1)),
    verified_at INTEGER
) STRICT;
CREATE INDEX emails_user ON emails (user_id);
-- At most one primary address per user.
CREATE UNIQUE INDEX emails_one_primary ON emails (user_id) WHERE is_primary = 1;

-- Web sessions. The cookie holds a random token; only its hash is stored.
CREATE TABLE sessions (
    id_hash      BLOB    PRIMARY KEY CHECK (length(id_hash) = 32),
    user_id      INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at   INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL
) STRICT, WITHOUT ROWID;
CREATE INDEX sessions_user ON sessions (user_id);

-- Personal access tokens (FR-AUTH-011, 012).
CREATE TABLE access_tokens (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id      INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name         TEXT    NOT NULL,
    token_hash   BLOB    NOT NULL UNIQUE CHECK (length(token_hash) = 32),
    scopes       TEXT    NOT NULL,         -- space-separated, e.g. "repo:read repo:write"
    created_at   INTEGER NOT NULL,
    expires_at   INTEGER,                  -- NULL: never
    last_used_at INTEGER
) STRICT;
CREATE INDEX access_tokens_user ON access_tokens (user_id);

-- Single-use tokens. Only invites so far (FR-AUTH-002); magic links, password
-- resets and email verification join in Phase 5.
CREATE TABLE one_time_tokens (
    token_hash BLOB    PRIMARY KEY CHECK (length(token_hash) = 32),
    purpose    TEXT    NOT NULL CHECK (purpose IN ('invite')),
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    used_at    INTEGER
) STRICT, WITHOUT ROWID;

-- An owner with repositories can't be deleted until they are moved or deleted:
-- their directories on disk must go first.
CREATE TABLE repositories (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    owner_id   INTEGER NOT NULL REFERENCES owners (id) ON DELETE RESTRICT,
    name       TEXT    NOT NULL,          -- display form, as typed (FR-NAME-020)
    name_key   TEXT    NOT NULL,          -- ASCII-lowercased, without `.git`
    private    INTEGER NOT NULL DEFAULT 0 CHECK (private IN (0, 1)),   -- FR-REPO-010
    created_at INTEGER NOT NULL
) STRICT;
-- Also serves lookups by owner (it leads with owner_id).
CREATE UNIQUE INDEX repositories_owner_name ON repositories (owner_id, name_key);

-- Old names that point at a repository's ID, not its current name (FR-NAME-052).
-- Used from Phase 6 on.
CREATE TABLE repo_redirects (
    owner_id   INTEGER NOT NULL REFERENCES owners (id) ON DELETE CASCADE,
    name_key   TEXT    NOT NULL,
    repo_id    INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (owner_id, name_key)
) STRICT, WITHOUT ROWID;
CREATE INDEX repo_redirects_repo ON repo_redirects (repo_id);
