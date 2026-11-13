-- D1 Schema for Ralph Meet Chat
-- Run: wrangler d1 execute ralph-chat-db --local --file=d1_schema.sql

-- ── Core ─────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,               -- Ralph Auth user ID
    username TEXT NOT NULL UNIQUE,
    display_name TEXT,                 -- User's chosen display name (distinct from username)
    avatar_url TEXT,
    avatar_display TEXT,               -- JSON render instructions for square avatar frames
    banner_url TEXT,
    banner_content_type TEXT,
    nameplate_url TEXT,
    nameplate_content_type TEXT,
    profile_accent_color TEXT DEFAULT '#5865F2',
    profile_background_color TEXT DEFAULT '#161A22',
    profile_banner_color TEXT,
    display_name_style TEXT,
    theme_preference TEXT,
    theme_sync_enabled INTEGER NOT NULL DEFAULT 0,
    media_content_filter TEXT NOT NULL DEFAULT 'high',
    share_activity INTEGER NOT NULL DEFAULT 1, -- 0 hides game activity from others
    sound_settings TEXT,
    bio TEXT,
    pronouns TEXT,
    status TEXT DEFAULT 'online',      -- online | idle | dnd | offline
    custom_status TEXT,                -- e.g. "Today I learned..."
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT,
    suspended_at TEXT,                 -- site admin suspension; NULL = active
    suspended_reason TEXT,
    deleted_at TEXT                    -- set only on "Deleted user" rows (migration 045)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_username_lower
    ON users(lower(username));

CREATE TABLE IF NOT EXISTS user_avatar_uploads (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    file_key TEXT NOT NULL UNIQUE,
    avatar_url TEXT NOT NULL,
    content_type TEXT NOT NULL,
    pending_delete INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_user_avatar_uploads_user_created
    ON user_avatar_uploads(user_id, created_at DESC, id DESC);

CREATE TABLE IF NOT EXISTS user_identity_claims (
    auth_user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    legacy_user_id TEXT NOT NULL UNIQUE,
    email TEXT,
    match_method TEXT NOT NULL,
    claimed_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS servers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    owner_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    icon_url TEXT,
    invites_paused INTEGER NOT NULL DEFAULT 0,
    allow_public_shares INTEGER NOT NULL DEFAULT 1,
    show_source_in_shares INTEGER NOT NULL DEFAULT 0,
    allow_share_indexing INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS server_members (
    server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    joined_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (server_id, user_id)
);

CREATE TABLE IF NOT EXISTS roles (
    id TEXT PRIMARY KEY,
    server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    color TEXT,
    permissions INTEGER NOT NULL DEFAULT 0,
    position INTEGER NOT NULL DEFAULT 0,
    is_default INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS member_roles (
    server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    PRIMARY KEY (server_id, user_id, role_id)
);

CREATE TABLE IF NOT EXISTS categories (
    id TEXT PRIMARY KEY,
    server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    rank INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS channels (
    id TEXT PRIMARY KEY,
    server_id TEXT REFERENCES servers(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    channel_type TEXT NOT NULL DEFAULT 'text',  -- text | voice | dm
    category_id TEXT REFERENCES categories(id) ON DELETE SET NULL,
    position INTEGER NOT NULL DEFAULT 0,
    allow_public_shares INTEGER,
    voice_status_text TEXT,
    voice_status_media TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    author_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    reply_to_id TEXT REFERENCES messages(id) ON DELETE SET NULL,
    is_pinned INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT,
    embeds TEXT DEFAULT '[]',
    content_revision INTEGER NOT NULL DEFAULT 0,
    -- NULL = written by a person; 'call' = a DM call record (see migration 037)
    type TEXT,
    metadata TEXT
);

CREATE TABLE IF NOT EXISTS message_postprocessing_jobs (
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    revision INTEGER NOT NULL,
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    notify INTEGER NOT NULL DEFAULT 1,
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (message_id, revision)
);

-- ── Social ───────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS invites (
    code TEXT PRIMARY KEY,
    server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    channel_id TEXT REFERENCES channels(id) ON DELETE SET NULL,
    inviter_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    max_uses INTEGER,
    uses INTEGER NOT NULL DEFAULT 0,
    temporary INTEGER NOT NULL DEFAULT 0,
    expires_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS relationships (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    target_user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    type INTEGER NOT NULL,             -- 0=friend, 1=blocked, 2=pending_incoming, 3=pending_outgoing
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (user_id, target_user_id)
);

-- ── DMs & Read States ────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS dm_recipients (
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    last_read_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (channel_id, user_id)
);

CREATE TABLE IF NOT EXISTS read_states (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    last_read_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (user_id, channel_id)
);

-- ── Attachments & Reactions ──────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS attachments (
    id TEXT PRIMARY KEY,
    message_id TEXT REFERENCES messages(id) ON DELETE CASCADE,
    channel_id TEXT REFERENCES channels(id) ON DELETE CASCADE,
    soundboard_server_id TEXT REFERENCES servers(id) ON DELETE CASCADE,
    sound_name TEXT,
    sound_emoji TEXT,
    sound_volume REAL DEFAULT 1.0,
    filename TEXT NOT NULL,
    file_key TEXT NOT NULL,
    content_type TEXT,
    size_bytes INTEGER NOT NULL,
    is_nsfw INTEGER NOT NULL DEFAULT 0,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS message_reactions (
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    emoji TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (message_id, user_id, emoji)
);

CREATE TABLE IF NOT EXISTS gif_favorites (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    gif_id TEXT NOT NULL,
    title TEXT NOT NULL,
    alt_text TEXT,
    query TEXT,
    source_url TEXT NOT NULL,
    aspect_ratio REAL NOT NULL DEFAULT 1,
    preview_url TEXT NOT NULL,
    preview_width INTEGER NOT NULL,
    preview_height INTEGER NOT NULL,
    preview_size_bytes INTEGER NOT NULL DEFAULT 0,
    preview_content_type TEXT NOT NULL,
    send_url TEXT NOT NULL,
    send_width INTEGER NOT NULL,
    send_height INTEGER NOT NULL,
    send_size_bytes INTEGER NOT NULL DEFAULT 0,
    send_content_type TEXT NOT NULL,
    duration REAL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT,
    PRIMARY KEY (user_id, provider, gif_id)
);

CREATE TABLE IF NOT EXISTS myinstants_favorites (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    sound_id TEXT NOT NULL,
    title TEXT NOT NULL,
    url TEXT NOT NULL,
    color TEXT NOT NULL,
    sound_type TEXT DEFAULT 'myinstants',
    emoji TEXT,
    source_server_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (user_id, sound_id)
);

CREATE TABLE IF NOT EXISTS voice_status_media_assets (
    id TEXT PRIMARY KEY,
    server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    filename TEXT NOT NULL,
    file_key TEXT NOT NULL UNIQUE,
    content_type TEXT NOT NULL,
    preview_width INTEGER NOT NULL,
    preview_height INTEGER NOT NULL,
    size_bytes INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_voice_status_media_assets_server_created_at
    ON voice_status_media_assets(server_id, created_at DESC);

CREATE TABLE IF NOT EXISTS generated_emojis (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    shortcode TEXT NOT NULL,
    prompt TEXT NOT NULL,
    file_key TEXT UNIQUE,
    content_type TEXT,
    size_bytes INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'pending',
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_generated_emojis_user_created_at
    ON generated_emojis(user_id, created_at DESC);

CREATE TABLE IF NOT EXISTS collectible_catalog_cache (
    source TEXT PRIMARY KEY,
    payload TEXT NOT NULL,
    etag TEXT,
    category_count INTEGER NOT NULL DEFAULT 0,
    item_count INTEGER NOT NULL DEFAULT 0,
    synced_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS collectible_catalog_sync_status (
    source TEXT PRIMARY KEY,
    last_attempt_at TEXT,
    last_success_at TEXT,
    last_failure_at TEXT,
    last_error TEXT,
    last_outcome TEXT,
    consecutive_failures INTEGER NOT NULL DEFAULT 0,
    item_count INTEGER NOT NULL DEFAULT 0,
    category_count INTEGER NOT NULL DEFAULT 0,
    counts_json TEXT,
    last_trigger TEXT,
    last_duration_ms INTEGER
);

CREATE TABLE IF NOT EXISTS message_shares (
    id TEXT PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    source_message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    source_channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    source_server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    created_by TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    snapshot_content TEXT NOT NULL,
    snapshot_author TEXT NOT NULL,
    snapshot_attachments TEXT NOT NULL DEFAULT '[]',
    snapshot_embeds TEXT NOT NULL DEFAULT '[]',
    snapshot_reactions TEXT NOT NULL DEFAULT '[]',
    omitted_attachment_count INTEGER NOT NULL DEFAULT 0,
    reply_count INTEGER NOT NULL DEFAULT 0,
    allow_indexing INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'active',
    view_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT,
    revoked_at TEXT,
    deleted_at TEXT
);

CREATE TABLE IF NOT EXISTS server_bans (
    server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    reason TEXT,
    banned_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (server_id, user_id)
);

-- ── Notifications ────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS notifications (
    id           TEXT PRIMARY KEY,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    type         TEXT NOT NULL,          -- 'mention' | 'reply' | 'dm'
    channel_id   TEXT REFERENCES channels(id) ON DELETE CASCADE,
    server_id    TEXT REFERENCES servers(id) ON DELETE CASCADE,
    message_id   TEXT REFERENCES messages(id) ON DELETE CASCADE,
    from_user_id TEXT REFERENCES users(id) ON DELETE CASCADE,
    content      TEXT,                   -- preview snippet (first 200 chars)
    is_read      INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    dedupe_key   TEXT
);

CREATE TABLE IF NOT EXISTS notification_delivery_markers (
    dedupe_key TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    notification_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS notification_clear_watermarks (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    cleared_at TEXT NOT NULL
);

-- ── Push tokens ──────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS push_tokens (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token TEXT NOT NULL UNIQUE,          -- FCM registration token
    platform TEXT NOT NULL,              -- 'android' | 'ios'
    app_variant TEXT,                    -- 'development' | 'preview' | 'production'
    push_format TEXT,                    -- NULL = legacy notification push | 'native' = data-only, app renders
    session_id TEXT,                     -- Kova session that registered it; NULL = before migration 045
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ── Tiers, badges and stats (migration 039) ─────────────────────────────

-- A user's plan. No row means free; founder and admin grants never expire.
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

-- Badge awards; the definitions live in code (src/lib/badges.ts).
CREATE TABLE IF NOT EXISTS user_badges (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    badge_id TEXT NOT NULL,
    awarded_at TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('auto', 'admin', 'grant')),
    PRIMARY KEY (user_id, badge_id)
);

-- Account-level counters for the badge rules.
CREATE TABLE IF NOT EXISTS user_stats (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    voice_seconds INTEGER NOT NULL DEFAULT 0,  -- capped at 24 h per session
    stream_seconds INTEGER NOT NULL DEFAULT 0,
    messages_sent INTEGER NOT NULL DEFAULT 0,
    first_seen_at TEXT,
    last_active_at TEXT,
    updated_at TEXT NOT NULL
);

-- ── Indexes ──────────────────────────────────────────────────────────────

CREATE INDEX IF NOT EXISTS idx_messages_channel_id ON messages(channel_id);
CREATE INDEX IF NOT EXISTS idx_messages_author_id ON messages(author_id);
CREATE INDEX IF NOT EXISTS idx_messages_reply_to_id ON messages(reply_to_id);
CREATE INDEX IF NOT EXISTS idx_messages_created_at ON messages(channel_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_messages_history_cursor ON messages(channel_id, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_message_postprocessing_jobs_created
    ON message_postprocessing_jobs(created_at ASC);
CREATE INDEX IF NOT EXISTS idx_channels_server_id ON channels(server_id);
CREATE INDEX IF NOT EXISTS idx_channels_category_id ON channels(category_id);
CREATE INDEX IF NOT EXISTS idx_server_members_user_id ON server_members(user_id);
CREATE INDEX IF NOT EXISTS idx_invites_server_id ON invites(server_id);
CREATE INDEX IF NOT EXISTS idx_relationships_user_id ON relationships(user_id);
CREATE INDEX IF NOT EXISTS idx_relationships_target ON relationships(target_user_id);
CREATE INDEX IF NOT EXISTS idx_dm_recipients_user_id ON dm_recipients(user_id);
CREATE INDEX IF NOT EXISTS idx_attachments_message_id ON attachments(message_id);
CREATE INDEX IF NOT EXISTS idx_attachments_soundboard_server_id ON attachments(soundboard_server_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_message_reactions_message_id ON message_reactions(message_id);
CREATE INDEX IF NOT EXISTS idx_gif_favorites_user_created ON gif_favorites(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_message_shares_token ON message_shares(token);
CREATE INDEX IF NOT EXISTS idx_message_shares_created_by ON message_shares(created_by, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_message_shares_source_message ON message_shares(source_message_id);
CREATE INDEX IF NOT EXISTS idx_categories_server_id ON categories(server_id);
CREATE INDEX IF NOT EXISTS idx_myinstants_favorites_user_created ON myinstants_favorites(user_id, created_at DESC);

-- Composite index for requireChannelAccess() JOIN query
CREATE INDEX IF NOT EXISTS idx_server_members_composite ON server_members(user_id, server_id);

-- Attachment lookup by uploader
CREATE INDEX IF NOT EXISTS idx_attachments_user_id ON attachments(user_id);

-- Durable references for R2 objects whose deletion needs a later retry.
CREATE TABLE IF NOT EXISTS r2_cleanup_jobs (
    file_key TEXT PRIMARY KEY,
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    next_attempt_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Chunked attachment uploads (R2 multipart through the Worker binding).
-- One row per upload in flight: who started it, for which channel, the R2
-- key and multipart id the server chose, and what the client declared.
-- `status` is 'open' while parts arrive and 'completing' while the complete
-- route owns it. The hourly cron aborts rows past `expires_at`, and open
-- rows whose `last_activity_at` (create, status or a stored part) is over an
-- hour old: a closed tab's upload is gone within the hour.
CREATE TABLE IF NOT EXISTS pending_uploads (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    attachment_id TEXT NOT NULL,
    file_key TEXT NOT NULL,
    r2_upload_id TEXT NOT NULL,
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL,
    declared_size INTEGER NOT NULL,
    part_size INTEGER NOT NULL,
    part_count INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open', 'completing')),
    created_at TEXT NOT NULL,
    last_activity_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_pending_uploads_user ON pending_uploads(user_id, expires_at);
CREATE INDEX IF NOT EXISTS idx_pending_uploads_activity ON pending_uploads(status, last_activity_at);
CREATE INDEX IF NOT EXISTS idx_pending_uploads_expires ON pending_uploads(expires_at);

-- The parts R2 accepted, with the byte count the part route streamed, so the
-- complete route can sum the real size instead of trusting the declared one.
CREATE TABLE IF NOT EXISTS pending_upload_parts (
    upload_id TEXT NOT NULL REFERENCES pending_uploads(id) ON DELETE CASCADE,
    part_number INTEGER NOT NULL,
    etag TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    uploaded_at TEXT NOT NULL,
    PRIMARY KEY (upload_id, part_number)
);

-- Connections (migration 042): verified links to accounts on other services.
-- One external account per Yap account, one account per provider per user.
CREATE TABLE IF NOT EXISTS user_connections (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    external_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    profile_url TEXT,
    avatar_url TEXT,
    show_on_profile INTEGER NOT NULL DEFAULT 1 CHECK (show_on_profile IN (0, 1)),
    metadata TEXT NOT NULL DEFAULT '{}',
    verified_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (user_id, provider),
    UNIQUE (provider, external_id)
);

-- A link in flight (start route to provider callback), consumed once.
CREATE TABLE IF NOT EXISTS connection_link_states (
    state_hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    binding_hash TEXT NOT NULL,
    return_to TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    created_at TEXT NOT NULL
);

-- Read states per user
CREATE INDEX IF NOT EXISTS idx_read_states_user_id ON read_states(user_id);

-- Ban lookups
CREATE INDEX IF NOT EXISTS idx_server_bans_server ON server_bans(server_id);

-- Notification inbox lookup
CREATE INDEX IF NOT EXISTS idx_notifications_user ON notifications(user_id, is_read, created_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS idx_notifications_dedupe_key
    ON notifications(dedupe_key) WHERE dedupe_key IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_notification_delivery_markers_message
    ON notification_delivery_markers(message_id);

-- Push fan-out per recipient
CREATE INDEX IF NOT EXISTS idx_push_tokens_user_id ON push_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_push_tokens_user_session ON push_tokens(user_id, session_id);

-- Billing webhooks find a plan by its Stripe customer
CREATE INDEX IF NOT EXISTS idx_user_entitlements_stripe_customer
    ON user_entitlements(stripe_customer_id)
    WHERE stripe_customer_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_user_badges_badge ON user_badges(badge_id);
CREATE INDEX IF NOT EXISTS idx_user_stats_voice ON user_stats(voice_seconds);
CREATE INDEX IF NOT EXISTS idx_connection_link_states_expires ON connection_link_states(expires_at);

-- ── Audit Log ────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS server_audit_logs (
    id TEXT PRIMARY KEY,
    server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    actor_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    action_type TEXT NOT NULL,
    target_id TEXT,
    changes TEXT, -- JSON string representing before/after states
    reason TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_server_audit_logs_server_time ON server_audit_logs(server_id, created_at DESC);

-- ── Channel Overrides ────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS channel_permission_overrides (
    id TEXT PRIMARY KEY,
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    target_id TEXT NOT NULL, -- role_id or user_id
    target_type TEXT NOT NULL CHECK(target_type IN ('role', 'user')),
    allow INTEGER NOT NULL DEFAULT 0,
    deny INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_channel_overrides_unique ON channel_permission_overrides(channel_id, target_id);

-- ── Site admin (migration 041) ─────────────────────────────────────────

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

CREATE INDEX IF NOT EXISTS idx_users_suspended ON users(suspended_at) WHERE suspended_at IS NOT NULL;

CREATE TABLE IF NOT EXISTS site_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    updated_by TEXT
);

CREATE TABLE IF NOT EXISTS scheduled_job_runs (
    name TEXT PRIMARY KEY,
    cron TEXT NOT NULL,
    ok INTEGER NOT NULL,
    duration_ms INTEGER NOT NULL,
    error TEXT,
    ran_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS daily_activity (
    day TEXT NOT NULL,
    user_id TEXT NOT NULL,
    messages INTEGER NOT NULL DEFAULT 0,
    voice_seconds INTEGER NOT NULL DEFAULT 0,
    stream_seconds INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (day, user_id)
);

CREATE INDEX IF NOT EXISTS idx_daily_activity_user ON daily_activity(user_id, day);

CREATE INDEX IF NOT EXISTS idx_users_created_at ON users(created_at);

CREATE INDEX IF NOT EXISTS idx_attachments_created_at ON attachments(created_at);

CREATE INDEX IF NOT EXISTS idx_messages_created_at_global ON messages(created_at);

-- Profile boards, Activity, guestbook and wishlist (migration 043).
CREATE TABLE IF NOT EXISTS profile_boards (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    board TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);

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

CREATE TABLE IF NOT EXISTS profile_guestbook_blocks (
    owner_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    blocked_user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    PRIMARY KEY (owner_id, blocked_user_id)
);

-- Deadlock widget career stats (migration 044).
CREATE TABLE IF NOT EXISTS deadlock_snapshots (
    account_id INTEGER PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'ok', 'empty', 'protected')),
    digest TEXT,
    fetched_at INTEGER,
    newest_match_id INTEGER
);

CREATE INDEX IF NOT EXISTS idx_deadlock_snapshots_user
    ON deadlock_snapshots(user_id);

CREATE TABLE IF NOT EXISTS deadlock_fetch_state (
    account_id INTEGER PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    next_due_at INTEGER NOT NULL,
    retry_at INTEGER,
    fail_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    attempted_at INTEGER,
    signal TEXT,
    history_at INTEGER,
    live_since INTEGER,
    live_until INTEGER,
    queued_at INTEGER,
    post_game_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_deadlock_fetch_state_due
    ON deadlock_fetch_state(next_due_at);
CREATE INDEX IF NOT EXISTS idx_deadlock_fetch_state_user
    ON deadlock_fetch_state(user_id);

-- Account deletion with a 30-day grace period (migration 045).
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
