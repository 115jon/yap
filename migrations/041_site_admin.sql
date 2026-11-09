-- The site-admin console (SITE_ADMIN_USER_IDS, src/lib/site-admin.ts).
--
-- admin_audit_log: every mutating admin action, written in the same batch as
-- the change it records. No foreign keys on purpose: the record outlives the
-- account it is about (and the admin who made it). `before_json` and
-- `after_json` are the row as the action read it and as it left it, `cf_ray`
-- ties the entry to the request in Workers logs.
CREATE TABLE IF NOT EXISTS admin_audit_log (
    id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL,
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT,
    before_json TEXT,
    after_json TEXT,
    cf_ray TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_admin_audit_log_time ON admin_audit_log(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_admin_audit_log_target ON admin_audit_log(target_type, target_id, created_at DESC);

-- Suspension: a suspended account's sign-in is refused at the auth path
-- (requireAuth and the edge's signedInUserId answer 403 ACCOUNT_SUSPENDED).
-- NULL means not suspended.
ALTER TABLE users ADD COLUMN suspended_at TEXT;
ALTER TABLE users ADD COLUMN suspended_reason TEXT;

CREATE INDEX IF NOT EXISTS idx_users_suspended ON users(suspended_at) WHERE suspended_at IS NOT NULL;

-- Site settings: feature flags, kill switches and the announcement banner,
-- one JSON value per key (src/lib/site-flags.ts). Read through a short
-- in-isolate cache, a missing key is the default (everything on, no banner).
CREATE TABLE IF NOT EXISTS site_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    updated_by TEXT
);

-- The cron jobs' last run, one row per job name (custom-server-entry.ts
-- scheduled()). Overwritten each run, the console reads it.
CREATE TABLE IF NOT EXISTS scheduled_job_runs (
    name TEXT PRIMARY KEY,
    cron TEXT NOT NULL,
    ok INTEGER NOT NULL,
    duration_ms INTEGER NOT NULL,
    error TEXT,
    ran_at TEXT NOT NULL
);

-- A light daily rollup for the console's charts: per user and UTC day, the
-- messages written and the voice and stream seconds of sessions that ended
-- that day. Written beside the user_stats writes (stats.service.ts), the
-- tables it summarises keep no per-day history for voice.
CREATE TABLE IF NOT EXISTS daily_activity (
    day TEXT NOT NULL,
    user_id TEXT NOT NULL,
    messages INTEGER NOT NULL DEFAULT 0,
    voice_seconds INTEGER NOT NULL DEFAULT 0,
    stream_seconds INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (day, user_id)
);

CREATE INDEX IF NOT EXISTS idx_daily_activity_user ON daily_activity(user_id, day);

-- Charts read signups and uploads by day.
CREATE INDEX IF NOT EXISTS idx_users_created_at ON users(created_at);
CREATE INDEX IF NOT EXISTS idx_attachments_created_at ON attachments(created_at);
CREATE INDEX IF NOT EXISTS idx_messages_created_at_global ON messages(created_at);
