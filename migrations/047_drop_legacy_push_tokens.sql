-- Every push token is the native format now. Rows with push_format NULL
-- were registered by the retired Expo app, which cannot render data-only
-- pushes: delete them. The rest are already 'native' (the only other value
-- /api/push-tokens accepts), and upsertPushToken writes 'native' from now on.
DELETE FROM push_tokens WHERE push_format IS NULL;
