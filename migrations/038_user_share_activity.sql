-- "Share my activity": others see the game a user is playing (game activity
-- on presence). On by default, as on Discord; invisible status hides it too.
ALTER TABLE users ADD COLUMN share_activity INTEGER NOT NULL DEFAULT 1;
