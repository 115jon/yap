-- A link a native app started (src/services/connection.service.ts,
-- holdLinkResult): the provider verified an account in the system browser,
-- but nothing is linked until the app that started it claims the result
-- with its own session (POST /api/connections/:provider/complete). The
-- browser only carries a one-time code back to the app; `code_hash` is its
-- SHA-256. A claim by any other account is refused, so a begin URL opened
-- by someone else can never link their account to the starter's (or the
-- starter's to theirs). Rows live ten minutes; expired ones are swept on
-- the next hold.
CREATE TABLE IF NOT EXISTS connection_link_results (
    code_hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    account TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_connection_link_results_expires ON connection_link_results(expires_at);
