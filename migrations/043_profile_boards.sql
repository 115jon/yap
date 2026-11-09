-- Profile boards: widgets, the Activity timeline, the guestbook and the
-- wishlist (profile widgets spec, 2026-11-09).
--
-- profile_boards: one JSON document per user (src/lib/profile-board.ts
-- ProfileBoard), always read and written whole. `version` goes up by one on
-- every save; a save names the version it started from and gets a 409 when
-- the row has moved on. No row means an empty board at version 0.
CREATE TABLE IF NOT EXISTS profile_boards (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    board TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);

-- profile_prefs: the owner's Activity and guestbook settings and the
-- wishlist, apart from the board so changing them never bumps its version.
-- No row means the defaults (30 days, nothing hidden, guestbook on, empty
-- wishlist).
CREATE TABLE IF NOT EXISTS profile_prefs (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    activity_retention_days INTEGER NOT NULL DEFAULT 30
        CHECK (activity_retention_days IN (7, 30, 90)),
    -- JSON array of activity kinds shown only to the owner.
    activity_hidden_kinds TEXT NOT NULL DEFAULT '[]',
    guestbook_enabled INTEGER NOT NULL DEFAULT 1,
    -- JSON array of WishlistItem.
    wishlist TEXT NOT NULL DEFAULT '[]',
    updated_at TEXT NOT NULL
);

-- profile_activity: the server-written timeline. Ids are deterministic per
-- event (game:<user>:<start>:<name>, voice:<user>:<session>, ...), and every
-- write is INSERT OR IGNORE, so a retry, a resumed socket or two gateways
-- never write one event twice. Rows past the owner's retention are deleted
-- by the daily cron, not hidden.
CREATE TABLE IF NOT EXISTS profile_activity (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind TEXT NOT NULL
        CHECK (kind IN ('game', 'voice', 'stream', 'listen', 'badge', 'server_join')),
    ref TEXT,
    data TEXT NOT NULL DEFAULT '{}',
    started_at TEXT NOT NULL,
    ended_at TEXT,
    seconds INTEGER,
    hidden INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_profile_activity_user
    ON profile_activity(user_id, started_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_profile_activity_started
    ON profile_activity(started_at);

-- game_sessions: play time per user, UTC day and game, for Top games and
-- Now playing's totals. Kept apart from profile_activity, whose retention is
-- short. `game_key` is the lower-cased name.
CREATE TABLE IF NOT EXISTS game_sessions (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    day TEXT NOT NULL,
    game_key TEXT NOT NULL,
    name TEXT NOT NULL,
    icon_url TEXT,
    seconds INTEGER NOT NULL DEFAULT 0,
    sessions INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, day, game_key)
);

CREATE INDEX IF NOT EXISTS idx_game_sessions_user_day
    ON game_sessions(user_id, day);

-- profile_guestbook: notes post immediately. The owner deletes any note;
-- an author deletes their own.
CREATE TABLE IF NOT EXISTS profile_guestbook (
    id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    author_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    body TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_profile_guestbook_owner
    ON profile_guestbook(owner_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_profile_guestbook_author
    ON profile_guestbook(owner_id, author_id, created_at);

-- People the owner blocked from their guestbook (and nothing else).
CREATE TABLE IF NOT EXISTS profile_guestbook_blocks (
    owner_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    blocked_user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    PRIMARY KEY (owner_id, blocked_user_id)
);
