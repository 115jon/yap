-- The Kova session that registered each push token (M2 of the privacy audit).
-- Signing a session out (DELETE /api/sessions, DELETE /api/push-tokens)
-- deletes the tokens it registered, so a revoked device stops getting the
-- account's pushes.
--
-- Rows from before this column have session_id NULL: nothing says which
-- device they belong to. Revoking any of the user's sessions deletes them
-- too; the user's live devices register again on their next start (both apps
-- register once per signed-in session) and get a row bound to their session.
ALTER TABLE push_tokens ADD COLUMN session_id TEXT;

CREATE INDEX IF NOT EXISTS idx_push_tokens_user_session
    ON push_tokens(user_id, session_id);
