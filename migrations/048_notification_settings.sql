-- Per-server, per-channel and per-DM notification levels and mutes
-- (src/lib/notification-levels.ts has the rules).
--
-- One row per user and target. target_type 'server' names a server;
-- 'channel' names a server channel or a DM channel. No foreign key on
-- target_id since it names either table; a row for a server the user left
-- or a channel that was deleted is never read again.
--
-- level: 'all' | 'mentions' | 'none', or NULL to inherit (a channel takes its
-- server's level, then the default: mentions in servers, all in DMs).
-- muted: 1 while muted; muted_until is when a timed mute ends (ISO), NULL
-- for "until I turn it back on". A row with level NULL and muted 0 is
-- deleted rather than kept.
--
-- Numbered 048: 047 is taken by an open change (drop legacy push tokens).
CREATE TABLE IF NOT EXISTS notification_settings (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    target_type TEXT NOT NULL CHECK (target_type IN ('server', 'channel')),
    target_id TEXT NOT NULL,
    level TEXT CHECK (level IS NULL OR level IN ('all', 'mentions', 'none')),
    muted INTEGER NOT NULL DEFAULT 0,
    muted_until TEXT,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (user_id, target_type, target_id)
);

-- "All messages" fan-out: who asked for every message of a channel or server.
CREATE INDEX IF NOT EXISTS idx_notification_settings_target
    ON notification_settings(target_type, target_id);
