-- The Deadlock profile widget's career stats (src/services/deadlock/).
-- A server job reads deadlock-api.com for Steam accounts whose owner has the
-- widget on their board, reduces the answers to a small digest
-- (src/lib/deadlock.ts DeadlockDigest, a few KB) and keeps it here. Clients
-- never call deadlock-api.com: the board route sends the digest, and the art
-- is mirrored into R2 under game-assets/.
--
-- Neither table keeps a match history, another player's id or the Steam
-- friends list. Unlinking Steam deletes both rows (connection.service).

-- The last good digest per account. `status` says what the plate shows:
-- ok, empty (no matches at the source), protected (the account asked
-- deadlock-api.com to delete its data; the API answers 403 and the digest is
-- dropped). A row without a digest and status 'pending' is a first fetch
-- still on its way.
CREATE TABLE IF NOT EXISTS deadlock_snapshots (
    account_id INTEGER PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'ok', 'empty', 'protected')),
    digest TEXT,
    -- Epoch ms the digest was last confirmed against the API.
    fetched_at INTEGER,
    -- The newest match in the digest: a refetch of the history and the
    -- match metadata happens only when this moves.
    newest_match_id INTEGER
);

CREATE INDEX IF NOT EXISTS idx_deadlock_snapshots_user
    ON deadlock_snapshots(user_id);

-- When and how to fetch each tracked account. A row exists while the
-- owner has a linked Steam account and the widget on their board (the
-- scheduled job reconciles both ways).
CREATE TABLE IF NOT EXISTS deadlock_fetch_state (
    account_id INTEGER PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Epoch ms. The 10-minute sweep fetches rows that are due.
    next_due_at INTEGER NOT NULL,
    -- Epoch ms before which nothing is sent for this account (Retry-After
    -- and per-account backoff).
    retry_at INTEGER,
    fail_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    attempted_at INTEGER,
    -- What the cheap batch calls said last time
    -- ("<rank match>:<matches>:<last played>"); a change means a new match.
    signal TEXT,
    -- Epoch ms the full history was last read (re-read at least daily).
    history_at INTEGER,
    -- Presence: Deadlock seen running since / until (epoch ms). While the
    -- owner plays, and for two hours after, the account is due every 10 min.
    live_since INTEGER,
    live_until INTEGER,
    -- Epoch ms an on-demand fetch was queued (a viewed stale plate), so
    -- many viewers queue one fetch.
    queued_at INTEGER,
    -- Epoch ms post-game checks were last queued: one pair per ten minutes,
    -- so a reconnect or a flapping game cannot flood the queue.
    post_game_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_deadlock_fetch_state_due
    ON deadlock_fetch_state(next_due_at);
CREATE INDEX IF NOT EXISTS idx_deadlock_fetch_state_user
    ON deadlock_fetch_state(user_id);
