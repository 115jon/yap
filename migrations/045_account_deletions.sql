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
    last_error TEXT
);

CREATE INDEX IF NOT EXISTS idx_account_deletions_reserved_username
    ON account_deletions(reserved_username);

CREATE INDEX IF NOT EXISTS idx_account_deletions_due
    ON account_deletions(delete_after);

-- "Deleted user" rows: the author that purged accounts' messages keep. Set
-- only on those rows; no one signs in as them.
ALTER TABLE users ADD COLUMN deleted_at TEXT;
