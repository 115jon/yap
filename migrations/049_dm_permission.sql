-- "Who can DM you" (src/services/dm-permission.service.ts has the rules).
--
-- everyone:            anyone can start a DM with the user (the default, and
--                      what every account had before this column).
-- friends_and_servers: only friends and people who share a server with them.
-- nobody:              no one can start a new DM with them.
--
-- The setting gates new conversations only: a DM the user has written in
-- (a message, or a call they placed) stays open under every setting. A block
-- either way closes a DM whatever this says; server admins get no bypass.
--
-- Numbered 049: 047 is taken by an open change, 048 by notification settings.
ALTER TABLE users ADD COLUMN dm_permission TEXT NOT NULL DEFAULT 'everyone'
    CHECK (dm_permission IN ('everyone', 'friends_and_servers', 'nobody'));

-- "Has the recipient written in this DM": one index probe per send, reaction,
-- typing event or ring into a DM whose recipient limits who can DM them.
CREATE INDEX IF NOT EXISTS idx_messages_channel_author
    ON messages(channel_id, author_id);
