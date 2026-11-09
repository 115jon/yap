//! Upload size limits and the "too big" copy (the shared too-big spec, which
//! the web implements word for word): one constant per kind, read from the
//! server's routes; sizes in MB of 1024 x 1024 as the server counts them;
//! the cards each picker shows; and the server's own number out of a 413.
//!
//! A file over its limit is never uploaded. A 413 can still come back when
//! the server's limit changed, so [`server_limit`] reads the number it
//! names and [`server_card`] says it.

use serde_json::{json, Value};

/// One megabyte as the server counts it (`MB = 1024 * 1024`).
pub const MB: u64 = 1024 * 1024;

/// `avatar-upload.ts` (`MAX_IMAGE_SIZE`, image-validation.ts).
pub const AVATAR: u64 = 8 * MB;
/// `profile-assets/manage.ts` (`MAX_PROFILE_BANNER_SIZE = MAX_IMAGE_SIZE`).
pub const BANNER: u64 = 8 * MB;
/// `servers/icon-upload.ts` (`MAX_IMAGE_SIZE`).
pub const SERVER_ICON: u64 = 8 * MB;
/// `channels/$id/messages/upload.ts` (`DEFAULT_UPLOAD_LIMIT_BYTES`).
pub const ATTACHMENT: u64 = 25 * MB;
/// `gif-picker.ts` (`MAX_GIF_UPLOAD_BYTES`, checked by `messages/gif.ts`).
pub const GIF: u64 = 25 * MB;
/// `voice-status-media.ts` (`MAX_VOICE_STATUS_MEDIA_UPLOAD_BYTES`).
pub const VIBE: u64 = 25 * MB;
/// `camera-background-validation.ts` (`CAMERA_BACKGROUND_UPLOAD_LIMIT_BYTES`).
pub const BACKGROUND: u64 = 25 * MB;
/// `profile-asset-validation.ts` (`MAX_PROFILE_NAMEPLATE_SIZE`).
pub const NAMEPLATE: u64 = 25 * MB;
/// `voice/soundboard.ts` (`MAX_SOUNDBOARD_UPLOAD_BYTES`; `upload.ts` with
/// `purpose=soundboard`).
pub const SOUND: u64 = 50 * MB;

/// What one attachment through the single-request Worker path may weigh,
/// on every tier (`WORKER_UPLOAD_CAP`): bigger files go up in parts.
pub const WORKER_UPLOAD_CAP: u64 = 25 * MB;

/// The chunked direct-to-R2 path is live (`CHUNKED_UPLOADS_ENABLED`), so
/// the client offers its limit (500 MB attachments with `bigUploads`).
pub const CHUNKED_UPLOADS_ENABLED: bool = true;

/// Attachments this size or larger go up in parts
/// (`MULTIPART_THRESHOLD`); smaller ones keep the single request.
pub const MULTIPART_THRESHOLD: u64 = 16 * MB;

/// The part size the server hands out (`MULTIPART_PART_SIZE`, 8 MiB).
pub const MULTIPART_PART_SIZE: u64 = 8 * MB;

/// The largest attachment any plan may send (`MAX_ATTACHMENT_BYTES`).
pub const MAX_ATTACHMENT_BYTES: u64 = 500 * MB;

/// What is being uploaded; each kind has its limit and its card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Attachment,
    Avatar,
    Banner,
    Sound,
    Gif,
    Vibe,
    Background,
    Nameplate,
    ServerIcon,
}

impl Kind {
    /// The QML key (`"attachment"`, `"avatar"`, ...).
    pub fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "attachment" => Self::Attachment,
            "avatar" => Self::Avatar,
            "banner" => Self::Banner,
            "sound" => Self::Sound,
            "gif" => Self::Gif,
            "vibe" => Self::Vibe,
            "background" => Self::Background,
            "nameplate" => Self::Nameplate,
            "serverIcon" => Self::ServerIcon,
            _ => return None,
        })
    }

    pub fn limit(self) -> u64 {
        match self {
            Self::Attachment => ATTACHMENT,
            Self::Avatar => AVATAR,
            Self::Banner => BANNER,
            Self::Sound => SOUND,
            Self::Gif => GIF,
            Self::Vibe => VIBE,
            Self::Background => BACKGROUND,
            Self::Nameplate => NAMEPLATE,
            Self::ServerIcon => SERVER_ICON,
        }
    }

    pub fn fits(self, size: u64) -> bool {
        size <= self.limit()
    }

    /// The limit with the `bigUploads` perk where it raises one: the web's
    /// `BIG_UPLOAD_LIMITS`, which holds attachments (`MAX_ATTACHMENT_BYTES`,
    /// 500 MB), avatars and banners (`WORKER_UPLOAD_CAP`, 25 MB). Every
    /// other kind keeps its limit on every plan.
    pub fn big_limit(self) -> Option<u64> {
        match self {
            Self::Attachment => Some(MAX_ATTACHMENT_BYTES),
            Self::Avatar | Self::Banner => Some(WORKER_UPLOAD_CAP),
            _ => None,
        }
    }
}

/// `uploadLimitFor`: `kind`'s limit under `entitlement` at `now_ms`, as the
/// server enforces it. Through the Worker (`direct: false`) no kind goes
/// above `max(its free limit, WORKER_UPLOAD_CAP)`; `direct` is the chunked
/// path's limit (500 MB attachments with `bigUploads`).
pub fn upload_limit_for(
    entitlement: Option<&yap_protocol::tiers::Entitlement>,
    kind: Kind,
    direct: bool,
    now_ms: i64,
) -> u64 {
    use crate::entitlements::{has_perk, Perk};
    let base = kind.limit();
    let limit = match kind.big_limit() {
        Some(big) if has_perk(entitlement, Perk::BigUploads, now_ms) => big,
        _ => base,
    };
    if direct {
        limit
    } else {
        limit.min(base.max(WORKER_UPLOAD_CAP))
    }
}

/// `clientUploadLimitFor`: the limit the client offers, the chunked
/// path's while it is live.
pub fn client_upload_limit_for(
    entitlement: Option<&yap_protocol::tiers::Entitlement>,
    kind: Kind,
    now_ms: i64,
) -> u64 {
    upload_limit_for(entitlement, kind, CHUNKED_UPLOADS_ENABLED, now_ms)
}

/// `usesMultipartUpload`: a file this size takes the chunked path.
pub fn uses_multipart(size: u64) -> bool {
    size >= MULTIPART_THRESHOLD
}

/// `formatUploadProgress`: a chunked upload's bytes sent of its total, one
/// line in a 96 px tile: "60/150 MB". Whole MB from 10 MB up, one decimal
/// below ("0.4/16 MB"); the sent side rounds down so it never reads full
/// early, the total rounds to nearest.
pub fn format_progress(loaded: u64, total: u64) -> String {
    // Display only: f64 holds every size here closely enough.
    let mb = |bytes: u64, round: fn(f64) -> f64| {
        let value = bytes as f64 / MB as f64;
        if value >= 10.0 {
            format!("{}", round(value))
        } else {
            format!("{:.1}", round(value * 10.0) / 10.0)
        }
    };
    format!(
        "{}/{} MB",
        mb(loaded.min(total), f64::floor),
        mb(total, js_round)
    )
}

/// `Math.round` for the non-negative sizes here: halves go up.
fn js_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

/// MB rounded to one decimal (upload-limits.ts `oneDecimal`).
fn tenths(bytes: u64) -> f64 {
    // Display only: the precision lost converting to f64 is irrelevant.
    (bytes as f64 / MB as f64 * 10.0).round() / 10.0
}

/// A size: MB with one decimal (`31.2 MB`), upload-limits.ts
/// `formatUploadSize`.
pub fn format_size(bytes: u64) -> String {
    format!("{:.1} MB", tenths(bytes))
}

/// A limit: whole MB with no decimal (`25 MB`); one decimal only when it
/// is not a whole number (`formatUploadLimit`).
pub fn format_limit(bytes: u64) -> String {
    let mb = tenths(bytes);
    if mb.fract() == 0.0 {
        format!("{mb:.0} MB")
    } else {
        format!("{mb:.1} MB")
    }
}

/// How far `size` is over `limit`, rounded up to the next 0.1 MB, so a
/// file a few bytes over never reads "0.0 MB over" (`formatUploadOverage`).
pub fn format_over(size: u64, limit: u64) -> String {
    let over = size.saturating_sub(limit);
    // Tenths of a MB, rounded up, at least one.
    let tenths = (over.saturating_mul(10)).div_ceil(MB).max(1);
    format!("{}.{} MB", tenths / 10, tenths % 10)
}

/// The limit a 413's message names (`parseServerLimitBytes`): every upload
/// route answers "<X> too large (max 25MB)"; "max 8 MB" and "max 7.5MB"
/// read too, in any case. `None` when the message names none.
pub fn server_limit(message: &str) -> Option<u64> {
    let lower = message.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut from = 0;
    while let Some(found) = lower[from..].find("max") {
        let at = from + found;
        from = at + 3;
        // \bmax: not inside a word.
        if at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_') {
            continue;
        }
        let mut i = at + 3;
        if bytes.get(i) == Some(&b'.') {
            i += 1;
        }
        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        let start = i;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            continue;
        }
        if bytes.get(i) == Some(&b'.') && bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
            i += 1;
            while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
        }
        let number = &lower[start..i];
        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        let unit = lower.get(i..i + 2) == Some("mb")
            && !bytes
                .get(i + 2)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_');
        if !unit {
            continue;
        }
        let value: f64 = number.parse().ok()?;
        if value.is_finite() && value > 0.0 {
            // Rounded to whole bytes, as `Math.round(mb * MB)`.
            return Some((value * MB as f64).round() as u64);
        }
        return None;
    }
    None
}

/// A 413 as one sentence (api-errors.ts `describeApiError`): "Unable to
/// {action}. The server says the limit is 8 MB.", with the server's number,
/// else `fallback` (the client's limit for that upload, when known), else
/// "It's over the server's size limit."
pub fn server_refusal_text(message: &str, action: &str, fallback: Option<u64>) -> String {
    match server_limit(message).or(fallback) {
        Some(limit) => format!(
            "Unable to {action}. The server says the limit is {}.",
            format_limit(limit)
        ),
        None => format!("Unable to {action}. It's over the server's size limit."),
    }
}

/// The client's limit for an upload, by the action words its error copy
/// uses: the 413 sentence's fallback when the server names no limit. The
/// avatar and banner read as the free limit, the one every plan has; the
/// server's own number (8 or 25 MB) is what a 413 normally carries.
pub fn limit_for_action(action: &str) -> Option<u64> {
    Some(match action {
        "upload your avatar" => AVATAR,
        "upload your banner" => BANNER,
        "upload your nameplate" => NAMEPLATE,
        "upload the server icon" | "upload that icon" => SERVER_ICON,
        "upload that file" => ATTACHMENT,
        "upload that media" => VIBE,
        "upload that image" => BACKGROUND,
        "add that sound" => SOUND,
        _ => return None,
    })
}

/// Mascot poses (the spec's A, B, C).
pub const STUFFED: &str = "stuffed";
pub const SAD: &str = "sad";
pub const SQUASHED: &str = "squashed";

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The card's JSON for QML: `pose`, `headline`, `line` (plain), `lineRich`
/// (the file name in bold, escaped, for a StyledText), `hint`, `actions`
/// (ids: `remove`, `choose`, `pick`, `removeBig`), the meter's `size` and
/// `limit` (bytes, 0 for none) with their labels, and `alert`, the full
/// sentence screen readers get.
#[allow(clippy::too_many_arguments)]
fn card(
    pose: &str,
    headline: &str,
    line: &str,
    line_rich: &str,
    hint: &str,
    actions: &[&str],
    size: u64,
    limit: u64,
) -> Value {
    let alert = [headline, line, hint]
        .iter()
        .filter(|s| !s.is_empty())
        .map(|s| {
            let s = s.trim_end();
            if s.ends_with(['.', '!', '?']) {
                s.to_owned()
            } else {
                format!("{s}.")
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    json!({
        "pose": pose,
        "headline": headline,
        "line": line,
        "lineRich": line_rich,
        "hint": hint,
        "actions": actions,
        "size": size,
        "limit": limit,
        "sizeText": if size > 0 { format_size(size) } else { String::new() },
        "limitText": if limit > 0 { format_limit(limit) } else { String::new() },
        "alert": alert,
    })
}

/// The picker card for one file over `limit` (avatar, banner, sound, GIF,
/// vibe, background); `None` when it fits. `limit` is the user's own for
/// `kind` ([`client_upload_limit_for`]; `kind.limit()` where no plan
/// changes it). `is_gif` picks the avatar's "This GIF" over "This image".
pub fn picker_card(kind: Kind, size: u64, limit: u64, is_gif: bool) -> Option<Value> {
    if size <= limit {
        return None;
    }
    let (l, s) = (format_limit(limit), format_size(size));
    let (pose, headline, line) = match kind {
        Kind::Avatar => (
            SAD,
            "Yappy can't carry that one",
            format!(
                "This {} is {s}. Avatars can be up to {l}.",
                if is_gif { "GIF" } else { "image" }
            ),
        ),
        Kind::Banner => (
            SAD,
            "That banner's too heavy",
            format!("Banners can be up to {l}; this one is {s}."),
        ),
        Kind::Sound => (
            SQUASHED,
            "That sound won't fit",
            format!("Sounds can be up to {l}; this one is {s}."),
        ),
        Kind::Gif => (
            SQUASHED,
            "That GIF won't fit",
            format!("GIFs can be up to {l}; this one is {s}."),
        ),
        Kind::Vibe => (
            SQUASHED,
            "That one won't fit",
            format!("Vibes can be up to {l}; this one is {s}."),
        ),
        Kind::Background => (
            SQUASHED,
            "That picture won't fit",
            format!("Backgrounds can be up to {l}; this one is {s}."),
        ),
        // Not in the spec's table: the composer's wording.
        Kind::Attachment | Kind::Nameplate | Kind::ServerIcon => (
            STUFFED,
            "Too big to chew",
            format!("This file is {s}. Files can be up to {l}."),
        ),
    };
    Some(card(
        pose,
        headline,
        &line,
        &escape(&line),
        "",
        &["pick"],
        size,
        limit,
    ))
}

/// The card for a 413 that came back anyway (pose B): "The server turned
/// it away", "It says the limit is 8 MB." with the server's number, else
/// `fallback` (the client's limit). `action` is `remove` in the tray,
/// `pick` in a picker.
pub fn server_card(message: &str, fallback: u64, action: &str) -> Value {
    let limit = server_limit(message).unwrap_or(fallback);
    let headline = "The server turned it away";
    let line = format!("It says the limit is {}.", format_limit(limit));
    card(
        SAD,
        headline,
        &line,
        &escape(&line),
        "",
        &[action],
        0,
        limit,
    )
}

/// "The other 2 will still send." (empty when nothing else sends).
fn others(m: usize) -> String {
    if m == 0 {
        String::new()
    } else {
        format!(" The other {m} will still send.")
    }
}

/// The composer's drop overlay while files are dragged over it, from each
/// file's name and size against the user's attachment `limit`
/// ([`client_upload_limit_for`]); `None` when every one fits.
/// One over: "Too big to chew", its name, size and the limit, the hint and
/// the meter. Several over: "A few of these won't fit" with the counts.
pub fn drop_card(files: &[(String, u64)], limit: u64) -> Option<Value> {
    let over: Vec<&(String, u64)> = files.iter().filter(|(_, s)| *s > limit).collect();
    let l = format_limit(limit);
    match over.as_slice() {
        [] => None,
        [(name, size)] => {
            let s = format_size(*size);
            Some(card(
                STUFFED,
                "Too big to chew",
                &format!("{name} is {s}. Files can be up to {l}."),
                &format!("<b>{}</b> is {s}. Files can be up to {l}.", escape(name)),
                "Drop a smaller file, or post a link to it instead.",
                &[],
                *size,
                limit,
            ))
        }
        many => {
            let line = format!(
                "{} files are over the {l} limit.{}",
                many.len(),
                others(files.len() - many.len())
            );
            Some(card(
                STUFFED,
                "A few of these won't fit",
                &line,
                &escape(&line),
                "",
                &[],
                0,
                limit,
            ))
        }
    }
}

/// One oversized file in the composer's tray: the name as its title,
/// "31.2 MB · 6.2 MB over the 25 MB limit", Remove and Choose another.
pub fn tray_card(name: &str, size: u64, limit: u64) -> Value {
    let size_text = format_size(size);
    let over = format!(
        "{} over the {} limit",
        format_over(size, limit),
        format_limit(limit)
    );
    let line = format!("{size_text} · {over}");
    let mut card = card(
        STUFFED,
        name,
        &line,
        &escape(&line),
        "",
        &["remove", "choose"],
        size,
        limit,
    );
    // too-big-copy.ts: "{name} is {size}, {over} over the {limit} limit."
    card["alert"] = Value::from(format!("{name} is {size_text}, {over}."));
    card
}

/// Several oversized files in the tray, `n` of them, with `m` others that
/// still send: one card with "Remove the big ones".
pub fn tray_summary(n: usize, m: usize, limit: u64) -> Value {
    let line = format!(
        "{n} files are over the {} limit.{}",
        format_limit(limit),
        others(m)
    );
    card(
        STUFFED,
        "A few of these won't fit",
        &line,
        &escape(&line),
        "",
        &["removeBig"],
        0,
        limit,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_match_the_server() {
        assert_eq!(AVATAR, 8_388_608);
        assert_eq!(BANNER, 8_388_608);
        assert_eq!(ATTACHMENT, 26_214_400);
        assert_eq!(GIF, 26_214_400);
        assert_eq!(VIBE, 26_214_400);
        assert_eq!(BACKGROUND, 26_214_400);
        assert_eq!(SOUND, 52_428_800);
        assert_eq!(Kind::from_key("sound").map(Kind::limit), Some(SOUND));
        assert_eq!(Kind::from_key("banner").map(Kind::limit), Some(BANNER));
        assert_eq!(Kind::from_key("nope"), None);
        assert!(Kind::Attachment.fits(ATTACHMENT));
        assert!(!Kind::Attachment.fits(ATTACHMENT + 1));
    }

    #[test]
    fn sizes_are_mib_with_one_decimal() {
        assert_eq!(format_size(32_715_571), "31.2 MB");
        assert_eq!(format_size(25 * MB), "25.0 MB");
        assert_eq!(format_size(MB / 2), "0.5 MB");
        // 20.4 MiB, not 21.4 MB of 1000 x 1000.
        assert_eq!(format_size(21_390_950), "20.4 MB");
        assert_eq!(format_limit(25 * MB), "25 MB");
        assert_eq!(format_limit(8 * MB), "8 MB");
        assert_eq!(format_limit(MB * 3 / 2), "1.5 MB");
        assert_eq!(format_size(MB / 10), "0.1 MB");
        // Over rounds up to the next 0.1 MB, never "0.0 MB over".
        assert_eq!(format_over(25 * MB + 1, 25 * MB), "0.1 MB");
        assert_eq!(format_over(25 * MB, 25 * MB), "0.1 MB");
        assert_eq!(format_over(32_715_571, 25 * MB), "6.2 MB");
        assert_eq!(format_over(26 * MB + 1, 25 * MB), "1.1 MB");
        assert_eq!(format_over(35 * MB, 25 * MB), "10.0 MB");
    }

    #[test]
    fn reads_the_servers_number_from_a_413() {
        assert_eq!(server_limit("File too large (max 25MB)"), Some(25 * MB));
        assert_eq!(server_limit("Avatar too large (max 8MB)"), Some(8 * MB));
        assert_eq!(server_limit("Banner too large (max 8MB)"), Some(8 * MB));
        assert_eq!(server_limit("Vibe too large (max 25MB)"), Some(25 * MB));
        assert_eq!(server_limit("GIF too large (max 25MB)"), Some(25 * MB));
        assert_eq!(server_limit("too large (MAX 50 mb)"), Some(50 * MB));
        assert_eq!(server_limit("max. 8MB"), Some(8 * MB));
        assert_eq!(server_limit("max 7.5MB"), Some(7 * MB + MB / 2));
        // Only a "max N MB" counts, as the web's parser reads it.
        assert_eq!(
            server_limit("Voice status media must be 25 MB or smaller."),
            None
        );
        assert_eq!(server_limit("GIF too large to upload"), None);
        assert_eq!(server_limit("climax 9MB"), None);
        assert_eq!(server_limit("max 8MBps"), None);
        assert_eq!(server_limit("request failed"), None);
        assert_eq!(server_limit(""), None);
        assert_eq!(
            server_refusal_text(
                "Avatar too large (max 10MB)",
                "upload your avatar",
                Some(AVATAR)
            ),
            "Unable to upload your avatar. The server says the limit is 10 MB."
        );
        assert_eq!(
            server_refusal_text("request failed", "upload your avatar", Some(AVATAR)),
            "Unable to upload your avatar. The server says the limit is 8 MB."
        );
        assert_eq!(
            server_refusal_text("request failed", "upload that file", None),
            "Unable to upload that file. It's over the server's size limit."
        );
        let card = server_card("File too large (max 30MB)", ATTACHMENT, "remove");
        assert_eq!(card["pose"], SAD);
        assert_eq!(card["headline"], "The server turned it away");
        assert_eq!(card["line"], "It says the limit is 30 MB.");
        assert_eq!(card["limit"], 30 * MB);
        assert_eq!(card["actions"], json!(["remove"]));
        assert_eq!(
            server_card("request failed", ATTACHMENT, "pick")["line"],
            "It says the limit is 25 MB."
        );
    }

    #[test]
    fn picker_copy_matches_the_spec() {
        let line =
            |kind, size, gif| picker_card(kind, size, kind.limit(), gif).unwrap()["line"].clone();
        let size = 21_390_950; // 20.4 MB
        assert_eq!(
            line(Kind::Avatar, size, true),
            "This GIF is 20.4 MB. Avatars can be up to 8 MB."
        );
        assert_eq!(
            line(Kind::Avatar, size, false),
            "This image is 20.4 MB. Avatars can be up to 8 MB."
        );
        assert_eq!(
            line(Kind::Banner, 12 * MB, false),
            "Banners can be up to 8 MB; this one is 12.0 MB."
        );
        assert_eq!(
            line(Kind::Sound, 63 * MB, false),
            "Sounds can be up to 50 MB; this one is 63.0 MB."
        );
        assert_eq!(
            line(Kind::Gif, 30 * MB, true),
            "GIFs can be up to 25 MB; this one is 30.0 MB."
        );
        assert_eq!(
            line(Kind::Vibe, 30 * MB, false),
            "Vibes can be up to 25 MB; this one is 30.0 MB."
        );
        assert_eq!(
            line(Kind::Background, 30 * MB, false),
            "Backgrounds can be up to 25 MB; this one is 30.0 MB."
        );
        let card = picker_card(Kind::Avatar, size, AVATAR, true).unwrap();
        assert_eq!(card["headline"], "Yappy can't carry that one");
        assert_eq!(card["pose"], SAD);
        assert_eq!(card["actions"], json!(["pick"]));
        assert_eq!(
            card["alert"],
            "Yappy can't carry that one. This GIF is 20.4 MB. Avatars can be up to 8 MB."
        );
        assert_eq!(
            picker_card(Kind::Sound, SOUND + 1, SOUND, false).unwrap()["pose"],
            SQUASHED
        );
        assert!(picker_card(Kind::Avatar, AVATAR, AVATAR, false).is_none());
    }

    #[test]
    fn composer_copy_matches_the_spec() {
        let one = drop_card(&[("raid-clip.mp4".into(), 32_715_571)], ATTACHMENT).unwrap();
        assert_eq!(one["headline"], "Too big to chew");
        assert_eq!(
            one["line"],
            "raid-clip.mp4 is 31.2 MB. Files can be up to 25 MB."
        );
        assert_eq!(
            one["lineRich"],
            "<b>raid-clip.mp4</b> is 31.2 MB. Files can be up to 25 MB."
        );
        assert_eq!(
            one["hint"],
            "Drop a smaller file, or post a link to it instead."
        );
        assert_eq!(one["sizeText"], "31.2 MB");
        assert_eq!(one["limitText"], "25 MB");
        let files = [
            ("a.mp4".to_owned(), 30 * MB),
            ("b.mp4".to_owned(), 40 * MB),
            ("c.mp4".to_owned(), 26 * MB),
            ("d.png".to_owned(), MB),
            ("e.png".to_owned(), 2 * MB),
        ];
        let many = drop_card(&files, ATTACHMENT).unwrap();
        assert_eq!(many["headline"], "A few of these won't fit");
        assert_eq!(
            many["line"],
            "3 files are over the 25 MB limit. The other 2 will still send."
        );
        assert_eq!(
            drop_card(&files[..2], ATTACHMENT).unwrap()["line"],
            "2 files are over the 25 MB limit."
        );
        assert!(drop_card(&files[3..], ATTACHMENT).is_none());
        let rich = drop_card(&[("<i>x</i>.bin".into(), 30 * MB)], ATTACHMENT).unwrap();
        assert_eq!(
            rich["lineRich"],
            "<b>&lt;i&gt;x&lt;/i&gt;.bin</b> is 30.0 MB. Files can be up to 25 MB."
        );

        let tile = tray_card("raid-clip.mp4", 32_715_571, ATTACHMENT);
        assert_eq!(tile["headline"], "raid-clip.mp4");
        assert_eq!(tile["line"], "31.2 MB · 6.2 MB over the 25 MB limit");
        assert_eq!(tile["actions"], json!(["remove", "choose"]));
        assert_eq!(
            tile["alert"],
            "raid-clip.mp4 is 31.2 MB, 6.2 MB over the 25 MB limit."
        );
        assert_eq!(
            one["alert"],
            concat!(
                "Too big to chew. raid-clip.mp4 is 31.2 MB. Files can be up to 25 MB. ",
                "Drop a smaller file, or post a link to it instead."
            )
        );
        // One over among files that fit: still the one-file copy.
        let mixed = drop_card(
            &[("a.mp4".into(), 30 * MB), ("b.png".into(), MB)],
            ATTACHMENT,
        )
        .unwrap();
        assert_eq!(mixed["headline"], "Too big to chew");
        assert_eq!(
            tray_summary(3, 2, ATTACHMENT)["line"],
            "3 files are over the 25 MB limit. The other 2 will still send."
        );
        assert_eq!(
            tray_summary(3, 2, ATTACHMENT)["actions"],
            json!(["removeBig"])
        );
    }
    fn plan(tier: yap_protocol::tiers::Tier) -> yap_protocol::tiers::Entitlement {
        yap_protocol::tiers::Entitlement {
            tier,
            ..Default::default()
        }
    }

    #[test]
    fn upload_limits_follow_the_plan() {
        use yap_protocol::tiers::Tier;
        let (free, loud, founder) = (plan(Tier::Free), plan(Tier::Loud), plan(Tier::Founder));
        let now = 1_793_000_000_000;
        // The chunked path: 500 MB attachments with bigUploads.
        assert_eq!(
            client_upload_limit_for(None, Kind::Attachment, now),
            25 * MB
        );
        assert_eq!(
            client_upload_limit_for(Some(&free), Kind::Attachment, now),
            25 * MB
        );
        assert_eq!(
            client_upload_limit_for(Some(&loud), Kind::Attachment, now),
            500 * MB
        );
        assert_eq!(
            client_upload_limit_for(Some(&founder), Kind::Attachment, now),
            500 * MB
        );
        // The Worker path stays at its cap on every tier.
        assert_eq!(
            upload_limit_for(Some(&founder), Kind::Attachment, false, now),
            25 * MB
        );
        // Other kinds keep their limits, the 50 MB soundboard included.
        assert_eq!(
            upload_limit_for(Some(&founder), Kind::Sound, false, now),
            50 * MB
        );
        // Avatars and banners rise to 25 MB with bigUploads, through the
        // Worker path too (its cap).
        for kind in [Kind::Avatar, Kind::Banner] {
            assert_eq!(client_upload_limit_for(None, kind, now), 8 * MB);
            assert_eq!(client_upload_limit_for(Some(&free), kind, now), 8 * MB);
            assert_eq!(client_upload_limit_for(Some(&loud), kind, now), 25 * MB);
            assert_eq!(client_upload_limit_for(Some(&founder), kind, now), 25 * MB);
            assert_eq!(upload_limit_for(Some(&founder), kind, false, now), 25 * MB);
        }
        assert_eq!(
            client_upload_limit_for(Some(&founder), Kind::ServerIcon, now),
            8 * MB
        );
        assert_eq!(
            client_upload_limit_for(Some(&founder), Kind::Gif, now),
            25 * MB
        );
        // An expired Loud is free.
        let lapsed = yap_protocol::tiers::Entitlement {
            tier: Tier::Loud,
            expires_at: Some("2020-01-01T00:00:00Z".into()),
            ..Default::default()
        };
        assert_eq!(
            client_upload_limit_for(Some(&lapsed), Kind::Attachment, now),
            25 * MB
        );
        // The cards use the limit they are given.
        assert_eq!(
            tray_card("big.mkv", 600 * MB, 500 * MB)["line"],
            "600.0 MB · 100.0 MB over the 500 MB limit"
        );
        assert!(drop_card(&[("ok.mkv".into(), 60 * MB)], 500 * MB).is_none());
        assert_eq!(
            tray_summary(2, 0, 500 * MB)["line"],
            "2 files are over the 500 MB limit."
        );
    }

    #[test]
    fn chunked_threshold_and_progress_match_the_web() {
        assert!(!uses_multipart(16 * MB - 1));
        assert!(uses_multipart(16 * MB));
        assert_eq!(MULTIPART_PART_SIZE, 8_388_608);
        assert_eq!(format_progress(54 * MB, 150 * MB), "54/150 MB");
        assert_eq!(format_progress(60 * MB + MB / 2, 150 * MB), "60/150 MB");
        assert_eq!(format_progress(MB * 4 / 10, 16 * MB), "0.3/16 MB");
        assert_eq!(format_progress(MB / 2, 16 * MB), "0.5/16 MB");
        assert_eq!(format_progress(0, 150 * MB), "0.0/150 MB");
        // The sent side rounds down: never full early.
        assert_eq!(format_progress(150 * MB - 1, 150 * MB), "149/150 MB");
        assert_eq!(format_progress(MB * 99 / 100, 2 * MB), "0.9/2.0 MB");
        // Over the total reads as the total.
        assert_eq!(format_progress(200 * MB, 150 * MB), "150/150 MB");
        // The total rounds to nearest.
        assert_eq!(format_progress(0, 60 * MB + MB / 2), "0.0/61 MB");
    }
}
