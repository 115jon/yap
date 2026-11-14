-- Self-service account deletion with a 30-day grace period.
--
-- account_deletions: one row per account waiting to be deleted. Asking for
-- deletion writes the row, hides the profile (the users row shows "Deleted
-- user" until then; `snapshot` keeps what it showed before, and the servers
-- the account left, so a restore can put them back) and refuses every other
-- request from the account (403 ACCOUNT_PENDING_DELETION). Restoring deletes
-- the row. After `delete_after` the hourly purge
-- (src/services/account-deletion.service.ts) moves what other people keep
-- (messages, DMs, server audit entries) to a new "Deleted user" row
-- (`tombstone_id`), deletes the account's users row (its own data cascades)
-- and its files, and then this row. No foreign key: the row outlives the
-- users row it names by one statement.
--
-- `reserved_username` is the account's username, lowercased: the username
-- check and profile edits refuse it while the account can still be restored.
-- Not unique: a Kova sign-up does not ask Yap, so a new account may arrive
-- with the name anyway (a restore then picks a free variant).
CREATE TABLE IF NOT EXISTS account_deletions (
    user_id TEXT PRIMARY KEY,
    requested_at TEXT NOT NULL,
    delete_after TEXT NOT NULL,
    reserved_username TEXT NOT NULL,
    snapshot TEXT NOT NULL,
    tombstone_id TEXT,
    purge_started_at TEXT,
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    -- Why the purge is holding the account (it owns a server, or it was
    -- suspended after asking): set by the sweep, which then tries it last
    -- and logs it only when the reason changes.
    held_reason TEXT
);

CREATE INDEX IF NOT EXISTS idx_account_deletions_reserved_username
    ON account_deletions(reserved_username);

CREATE INDEX IF NOT EXISTS idx_account_deletions_due
    ON account_deletions(delete_after);

-- "Deleted user" rows: the author that purged accounts' messages keep. Set
-- only on those rows; no one signs in as them.
ALTER TABLE users ADD COLUMN deleted_at TEXT;

-- Bans and blocks outlive the account they name. Yap's user id is the Kova
-- id, so the same Kova account signing up again after the purge gets the
-- same id back; with `ON DELETE CASCADE` on the banned or blocked side, the
-- purge's `DELETE FROM users` would take every ban and block against the
-- account with it. These three tables are rebuilt without that foreign key
-- (SQLite cannot drop a constraint in place): create, copy, drop, rename,
-- then the indexes again. The side that made the ban or block keeps its
-- foreign key, so a server's or blocker's own deletion still clears them.
-- Nothing references these tables, so dropping them cascades nothing.

CREATE TABLE server_bans_046 (
    server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    -- No foreign key: a ban holds after the banned account is purged.
    user_id TEXT NOT NULL,
    reason TEXT,
    banned_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (server_id, user_id)
);
INSERT INTO server_bans_046 (server_id, user_id, reason, banned_by, created_at)
    SELECT server_id, user_id, reason, banned_by, created_at FROM server_bans;
DROP TABLE server_bans;
ALTER TABLE server_bans_046 RENAME TO server_bans;
CREATE INDEX IF NOT EXISTS idx_server_bans_server ON server_bans(server_id);

CREATE TABLE relationships_046 (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- No foreign key: a block (type 1) holds after the blocked account is
    -- purged. The purge deletes the other types itself.
    target_user_id TEXT NOT NULL,
    type INTEGER NOT NULL,             -- 0=friend, 1=blocked, 2=pending_incoming, 3=pending_outgoing
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (user_id, target_user_id)
);
INSERT INTO relationships_046 (user_id, target_user_id, type, created_at, updated_at)
    SELECT user_id, target_user_id, type, created_at, updated_at FROM relationships;
DROP TABLE relationships;
ALTER TABLE relationships_046 RENAME TO relationships;
CREATE INDEX IF NOT EXISTS idx_relationships_user_id ON relationships(user_id);
CREATE INDEX IF NOT EXISTS idx_relationships_target ON relationships(target_user_id);

CREATE TABLE profile_guestbook_blocks_046 (
    owner_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- No foreign key: the block holds after the blocked account is purged.
    blocked_user_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (owner_id, blocked_user_id)
);
INSERT INTO profile_guestbook_blocks_046 (owner_id, blocked_user_id, created_at)
    SELECT owner_id, blocked_user_id, created_at FROM profile_guestbook_blocks;
DROP TABLE profile_guestbook_blocks;
ALTER TABLE profile_guestbook_blocks_046 RENAME TO profile_guestbook_blocks;
