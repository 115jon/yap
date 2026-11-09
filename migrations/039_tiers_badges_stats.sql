-- Tiers, badges and account stats (tiers-badges spec, 2026-11-09).
--
-- user_entitlements: a user's plan, kept apart from the profile so billing
-- can change it without touching `users`. No row means free. Founder and
-- admin grants never expire (expires_at NULL).
CREATE TABLE IF NOT EXISTS user_entitlements (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    tier TEXT NOT NULL CHECK (tier IN ('free', 'loud', 'founder')),
    source TEXT NOT NULL CHECK (source IN ('grant', 'stripe', 'admin')),
    started_at TEXT NOT NULL,
    expires_at TEXT,
    stripe_customer_id TEXT,
    stripe_subscription_id TEXT,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_user_entitlements_stripe_customer
    ON user_entitlements(stripe_customer_id)
    WHERE stripe_customer_id IS NOT NULL;

-- user_badges: one row per award. Definitions live in code (src/lib/badges.ts).
CREATE TABLE IF NOT EXISTS user_badges (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    badge_id TEXT NOT NULL,
    awarded_at TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('auto', 'admin', 'grant')),
    PRIMARY KEY (user_id, badge_id)
);

CREATE INDEX IF NOT EXISTS idx_user_badges_badge ON user_badges(badge_id);

-- user_stats: account-level counters the badge rules read. Voice and stream
-- seconds are added by the realtime gateway once per session end (capped at
-- 24 h a session); messages_sent on message create.
CREATE TABLE IF NOT EXISTS user_stats (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    voice_seconds INTEGER NOT NULL DEFAULT 0,
    stream_seconds INTEGER NOT NULL DEFAULT 0,
    messages_sent INTEGER NOT NULL DEFAULT 0,
    first_seen_at TEXT,
    last_active_at TEXT,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_user_stats_voice ON user_stats(voice_seconds);

-- The founder grant: everyone registered when this ships is a founder, for
-- life, with the (unnumbered) Founder badge. Sign-ups after this are free.
INSERT OR IGNORE INTO user_entitlements
    (user_id, tier, source, started_at, expires_at, updated_at)
SELECT id, 'founder', 'grant', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), NULL,
       strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
FROM users;

INSERT OR IGNORE INTO user_badges (user_id, badge_id, awarded_at, source)
SELECT id, 'founder', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), 'grant'
FROM users;

-- Stats for everyone who already exists: messages they have written so far
-- (system messages, such as call records, are not theirs), first seen when
-- the account was created.
INSERT OR IGNORE INTO user_stats
    (user_id, voice_seconds, stream_seconds, messages_sent, first_seen_at,
     last_active_at, updated_at)
SELECT u.id, 0, 0, COALESCE(m.sent, 0), u.created_at, NULL,
       strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
FROM users u
LEFT JOIN (
    SELECT author_id, COUNT(*) AS sent
    FROM messages
    WHERE type IS NULL
    GROUP BY author_id
) m ON m.author_id = u.id;
