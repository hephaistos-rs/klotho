-- Phase 1: owners, users, repositories and redirects.
-- Timestamps are RFC 3339 text in UTC (FR-API-011).
-- AUTOINCREMENT guarantees an ID is never reused, even after the row with the
-- highest ID is deleted (FR-STOR-003): repository paths are derived from IDs.

-- Users and organisations share one namespace (FR-NAME-011), enforced by the
-- unique key. Organisations arrive in Phase 7.
CREATE TABLE owners (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    kind       TEXT    NOT NULL CHECK (kind IN ('user', 'org')),
    name       TEXT    NOT NULL,          -- display form, as typed (FR-NAME-020)
    name_key   TEXT    NOT NULL UNIQUE,   -- ASCII-lowercased name (FR-NAME-021, 022)
    created_at TEXT    NOT NULL
);

-- Account data goes here from Phase 2 on. The row shares its ID with the owner.
CREATE TABLE users (
    id INTEGER PRIMARY KEY REFERENCES owners (id) ON DELETE CASCADE
);

CREATE TABLE repositories (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    owner_id   INTEGER NOT NULL REFERENCES owners (id),
    name       TEXT    NOT NULL,
    name_key   TEXT    NOT NULL,
    created_at TEXT    NOT NULL
);

-- Uniqueness is enforced by the store, not by a check before the insert (FR-NAME-022).
CREATE UNIQUE INDEX repositories_owner_name_key ON repositories (owner_id, name_key);

-- Old names that point at a repository's ID, not its current name (FR-NAME-052).
-- Created now, used from Phase 6 on.
CREATE TABLE repo_redirects (
    owner_id   INTEGER NOT NULL REFERENCES owners (id),
    name_key   TEXT    NOT NULL,
    repo_id    INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    created_at TEXT    NOT NULL,
    PRIMARY KEY (owner_id, name_key)
);
