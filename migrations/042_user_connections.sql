-- Connections: verified links to accounts on other services (Steam first),
-- shown on the profile the way Discord shows them (src/services/connection.service.ts).
--
-- One row per linked account. `provider` names a provider module
-- (src/services/connections/), `external_id` is that service's stable account
-- id (a SteamID64 for Steam). A row exists only after the provider proved the
-- user owns the account (`verified_at`). One external account links to one Yap
-- account, and a Yap account links one account per provider.
-- `metadata` is provider JSON for later consumers (Steam: the 32-bit account
-- id that Deadlock's APIs use). Viewers only ever see provider, display name,
-- profile URL and avatar, and only while `show_on_profile` is on.
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

-- A link in flight: the start route writes one, the provider's callback
-- consumes it (DELETE ... RETURNING, so it works once). Only hashes are
-- stored: `state_hash` of the nonce in return_to, `binding_hash` of the
-- HttpOnly cookie that ties the callback to the browser that started it.
-- Rows live ten minutes, expired ones are swept on the next start.
CREATE TABLE IF NOT EXISTS connection_link_states (
    state_hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    binding_hash TEXT NOT NULL,
    return_to TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_connection_link_states_expires ON connection_link_states(expires_at);
