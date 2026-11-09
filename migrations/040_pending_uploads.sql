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
