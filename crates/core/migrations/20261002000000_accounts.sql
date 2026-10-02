-- Phase 2: accounts, sessions, tokens, invites and repository visibility
-- (docs/design/auth-flows.md, "Data model"). Secrets are stored only as
-- SHA-256 hashes (tokens) or Argon2id PHC strings (passwords).

ALTER TABLE users ADD COLUMN password_hash TEXT;   -- NULL: can't sign in with a password
ALTER TABLE users ADD COLUMN is_admin INTEGER NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN suspended_at TEXT;

CREATE TABLE emails (
    user_id     INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    address     TEXT    NOT NULL,          -- as typed
    address_key TEXT    NOT NULL UNIQUE,   -- lowercased
    verified_at TEXT,
    is_primary  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX emails_user ON emails (user_id);

-- Web sessions. The cookie holds a random token; only its hash is stored.
CREATE TABLE sessions (
    id_hash      BLOB    PRIMARY KEY,
    user_id      INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at   TEXT    NOT NULL,
    last_seen_at TEXT    NOT NULL
);
CREATE INDEX sessions_user ON sessions (user_id);

-- Personal access tokens (FR-AUTH-011, 012).
CREATE TABLE access_tokens (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id      INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name         TEXT    NOT NULL,
    token_hash   BLOB    NOT NULL UNIQUE,
    scopes       TEXT    NOT NULL,          -- space-separated, e.g. "repo:read repo:write"
    created_at   TEXT    NOT NULL,
    expires_at   TEXT,
    last_used_at TEXT
);
CREATE INDEX access_tokens_user ON access_tokens (user_id);

-- Single-use tokens. Only invites so far (FR-AUTH-002); magic links, password
-- resets and email verification join in Phase 5.
CREATE TABLE one_time_tokens (
    token_hash BLOB PRIMARY KEY,
    purpose    TEXT NOT NULL CHECK (purpose IN ('invite')),
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used_at    TEXT
);

-- FR-REPO-010.
ALTER TABLE repositories ADD COLUMN private INTEGER NOT NULL DEFAULT 0;
