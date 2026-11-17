//! Meeting rooms: the web's anonymous rooms (`/room/<slug>`,
//! `RoomPageClient.tsx`). Anyone with the link joins under a name they
//! pick, signed in or not; the server admits them as a public demo
//! session (`socket-admission.ts` `authorizePublicDemo`). Pure: reading a
//! pasted code or link, the rules for a slug and a name, a new slug, and
//! the invite link.

use crate::links::{self, Link};

/// The longest slug the server takes (`realtime-policy.ts`
/// `roomSlugPattern`: 1 to 200 characters).
const MAX_SLUG_LEN: usize = 200;

/// The longest name a guest picks, in characters.
pub const MAX_NAME_CHARS: usize = 32;

/// A slug the server admits as a public room (`isPublicDemoRoomSlug`): a
/// letter or digit, then letters, digits, `-` and `_`, at most 200 long;
/// never the global gateway's room, a server voice channel's or a DM
/// call's.
pub fn is_room_slug(slug: &str) -> bool {
    let mut chars = slug.chars();
    let starts = chars.next().is_some_and(|c| c.is_ascii_alphanumeric());
    starts
        && slug.len() <= MAX_SLUG_LEN
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        && slug != "global-gateway"
        && !slug.starts_with("voice-")
        && !slug.starts_with("dm-call-")
}

/// A typed code as the web's join box reads it (`HomePageClient.tsx`
/// `joinRoom`): trimmed, lowercased, spaces as dashes.
fn normalize_code(text: &str) -> String {
    text.trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

/// The room a paste names: a bare code, a room link on our site (with or
/// without `https://`), or a `yap://room/<slug>` link. `None` for anything
/// else, an invite link included.
pub fn room_slug(text: &str, web_origin: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let link = links::parse_link(text, web_origin).or_else(|| {
        // A link pasted without its scheme (`yap.example/room/x`).
        let host = web_origin.split_once("://").map_or("", |(_, h)| h);
        let host = host.trim_end_matches('/');
        let ours = !host.is_empty()
            && text
                .get(..host.len())
                .is_some_and(|start| start.eq_ignore_ascii_case(host))
            && text
                .get(host.len()..)
                .is_some_and(|rest| rest.starts_with('/'));
        if ours {
            let scheme = web_origin.split_once("://").map_or("https", |(s, _)| s);
            links::parse_link(&format!("{scheme}://{text}"), web_origin)
        } else {
            None
        }
    });
    match link {
        Some(Link::Room(slug)) => Some(slug),
        Some(_) => None,
        None if text.contains('/') || text.contains(':') => None,
        None => {
            let slug = normalize_code(text);
            is_room_slug(&slug).then_some(slug)
        }
    }
}

/// The name a guest joins under: trimmed, never empty, at most
/// [`MAX_NAME_CHARS`] characters.
pub fn guest_name(text: &str) -> Option<String> {
    let name = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!name.is_empty() && name.chars().count() <= MAX_NAME_CHARS).then_some(name)
}

/// A new room's slug from `seed` (`HomePageClient.tsx` `randomSlug`): six
/// and four base-36 characters, `k3x9q2-7f1a`.
pub fn slug_from_seed(seed: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut n = seed;
    let mut out = String::with_capacity(11);
    for i in 0..10 {
        if i == 6 {
            out.push('-');
        }
        out.push(char::from(DIGITS[(n % 36) as usize]));
        n /= 36;
    }
    out
}

/// A new room's slug, seeded from the process's random hasher keys and
/// the clock: as unguessable as the web's `Math.random` one.
pub fn new_room_slug() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    hasher.write_u128(nanos);
    slug_from_seed(hasher.finish())
}

/// The room's invite link on the site (`room-links.ts` `buildRoomLink`).
pub fn room_link(slug: &str, web_origin: &str) -> String {
    format!("{}/room/{slug}", web_origin.trim_end_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SITE: &str = "https://yap.fixture.invalid";

    #[test]
    fn a_bare_code_should_be_read_as_the_web_reads_it() {
        assert_eq!(
            room_slug("  Team Sync ", SITE).as_deref(),
            Some("team-sync")
        );
        assert_eq!(
            room_slug("k3x9q2-7f1a", SITE).as_deref(),
            Some("k3x9q2-7f1a")
        );
    }

    #[test]
    fn room_links_should_give_their_slug() {
        for text in [
            "https://yap.fixture.invalid/room/standup",
            "yap.fixture.invalid/room/standup",
            "YAP.fixture.invalid/room/standup/",
            "yap://room/standup",
            "yap-native://room/standup",
        ] {
            assert_eq!(room_slug(text, SITE).as_deref(), Some("standup"), "{text}");
        }
    }

    #[test]
    fn other_links_and_reserved_rooms_should_be_refused() {
        for text in [
            "",
            "https://yap.fixture.invalid/invite/abc",
            "https://elsewhere.invalid/room/standup",
            "voice-s1-c1",
            "dm-call-a-b",
            "global-gateway",
            "-leading",
            "a/b",
            "émoji",
        ] {
            assert_eq!(room_slug(text, SITE), None, "{text}");
        }
    }

    #[test]
    fn names_should_be_trimmed_and_bounded() {
        assert_eq!(
            guest_name("  Ada   Lovelace ").as_deref(),
            Some("Ada Lovelace")
        );
        assert_eq!(guest_name("   "), None);
        assert!(guest_name(&"a".repeat(MAX_NAME_CHARS)).is_some());
        assert_eq!(guest_name(&"a".repeat(MAX_NAME_CHARS + 1)), None);
    }

    #[test]
    fn new_slugs_should_be_rooms() {
        assert_eq!(slug_from_seed(0), "000000-0000");
        let slug = new_room_slug();
        assert_eq!(slug.len(), 11);
        assert!(is_room_slug(&slug));
        assert_ne!(slug, new_room_slug());
    }

    #[test]
    fn the_link_should_be_the_site_path() {
        assert_eq!(
            room_link("standup", "https://yap.fixture.invalid/"),
            "https://yap.fixture.invalid/room/standup"
        );
    }
}
