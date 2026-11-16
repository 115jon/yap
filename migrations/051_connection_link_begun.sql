-- When a native app's begin URL was first loaded (src/services/connection.service.ts,
-- checkLinkBinding): the URL is good again only for a short grace after
-- that (a browser that fetches a link twice), not for the state's whole
-- ten minutes. Null until the first load; web links never set it.
ALTER TABLE connection_link_states ADD COLUMN begun_at INTEGER;
