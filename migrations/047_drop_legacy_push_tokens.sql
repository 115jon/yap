-- Every push token is the native format now. Rows with push_format NULL
-- were registered by the retired Expo app, which cannot render data-only
-- pushes: delete them, and store the rest as 'native'.
DELETE FROM push_tokens WHERE push_format IS NULL;
