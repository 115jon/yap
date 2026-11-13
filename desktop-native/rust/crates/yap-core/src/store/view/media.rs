//! Attachments, reaction chips, link previews, the composer tray and the
//! GIF grid as display-ready values (MessageItem.tsx, ImageGrid.tsx,
//! LinkEmbed.tsx, AttachmentList.tsx, GifPickerModal.tsx).
//!
//! A message's attachments, reactions and embeds reach QML as one JSON
//! string role each (empty when there are none): QML parses it once when a
//! delegate shows the row, and the row diff stays a string compare.

use serde_json::{json, Value};
use yap_protocol::rest::{Attachment, Embed, GifItem, Reaction};

use super::{query_value, GifRow, GifStatusView, RowContext, UploadRow};
use crate::emoji;
use crate::gif_favorite;
use crate::store::state::{GifBrowse, Upload, UploadStatus};

/// The free attachment limit (upload.ts, 25 MiB); a plan with
/// `bigUploads` raises it ([`crate::limits::client_upload_limit_for`]).
pub const MAX_UPLOAD_BYTES: u64 = crate::limits::ATTACHMENT;

/// `formatFileSize` (MessageItem.tsx): 1024-based, one decimal, no GB.
pub fn format_file_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * 1024;
    // Display only: the precision lost converting to f64 is irrelevant.
    match bytes {
        b if b < KIB => format!("{b} B"),
        b if b < MIB => format!("{:.1} KB", b as f64 / KIB as f64),
        b => format!("{:.1} MB", b as f64 / MIB as f64),
    }
}

/// `image`, `video` (media.ts `isPlayableVideo`) or `file`.
pub fn media_kind(content_type: &str) -> &'static str {
    let mime = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if mime.starts_with("image/") {
        "image"
    } else if matches!(
        mime.as_str(),
        "video/mp4" | "video/webm" | "video/ogg" | "video/mp2t"
    ) {
        "video"
    } else {
        "file"
    }
}

fn host_of(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?.split(':').next()?;
    Some(host.to_ascii_lowercase())
}

fn is_web_url(url: &str) -> bool {
    host_of(url).is_some_and(|host| !host.is_empty())
}

/// One of the server's `/api/proxy-media` URLs, on any host.
fn is_proxy_url(url: &str) -> bool {
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return false;
    };
    let path = rest
        .find('/')
        .map_or("", |slash| &rest[slash..])
        .split(['?', '#'])
        .next()
        .unwrap_or_default();
    path == "/api/proxy-media"
}

/// A third-party picture or video through the server's media proxy, so its
/// host never sees this device's address (a posted link could otherwise
/// collect every viewer's). The API's own files and proxy URLs the server
/// built (signed, or the social form with its post) load as they are; an
/// unsigned one carries the session (`api::media_needs_auth`).
pub(crate) fn via_media_proxy(url: &str, api_base: &str) -> String {
    if !is_web_url(url) || crate::api::is_api_url(api_base, url) {
        return url.to_owned();
    }
    // A proxy path on another host goes to our API with its query: never
    // to that host, which only has to name a path like ours.
    if is_proxy_url(url) {
        let query = url.split_once('?').map_or("", |(_, q)| q);
        let query = query.split('#').next().unwrap_or_default();
        return if query.is_empty() {
            format!("{api_base}/api/proxy-media")
        } else {
            format!("{api_base}/api/proxy-media?{query}")
        };
    }
    format!("{api_base}/api/proxy-media?url={}", query_value(url))
}

/// A GIF provider's CDN (gif-picker.ts `getGifAttachmentProvider`).
fn is_gif_host(url: &str) -> bool {
    host_of(url).is_some_and(|host| {
        host == "tenor.com"
            || host.ends_with(".tenor.com")
            || host == "klipy.com"
            || host.ends_with(".klipy.com")
    })
}

/// Percent-encodes what a URL path cannot carry raw (the server puts the
/// file name in `file_url` unencoded).
fn encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        let keep = byte.is_ascii_alphanumeric() || b"-._~/!$&'()*+,;=:@".contains(&byte);
        if keep {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Where an attachment loads from: `url`, else `getAttachmentUrl(file_key)`,
/// made absolute against the API. A local `file:` preview passes through.
pub fn attachment_url(attachment: &Attachment, api_base: &str) -> String {
    let raw = attachment
        .url
        .as_deref()
        .filter(|u| !u.is_empty())
        .or(attachment.file_key.as_deref())
        .unwrap_or_default();
    // `qrc:` is the offline fixture's picture.
    if raw.starts_with("file:") || raw.starts_with("qrc:") {
        return raw.to_owned();
    }
    // A sent GIF keeps its provider's URL: load it through the proxy.
    if is_web_url(raw) {
        return via_media_proxy(raw, api_base);
    }
    if raw.is_empty() {
        return String::new();
    }
    let path = if raw.starts_with('/') {
        raw.to_owned()
    } else {
        format!("/api/{raw}")
    };
    format!("{api_base}{}", encode_path(&path))
}

/// A video attachment's star (MessageItem.tsx): a GIF (Klipy, Tenor, an
/// X GIF) saves as that GIF, any other mp4 as a clip; other videos have
/// none. The ids are the web's, so a star saved there shows here.
fn attachment_favorite(a: &Attachment, url: &str) -> Value {
    let mime = a.content_type.as_deref().unwrap_or_default();
    // `getAttachmentSourceUrl`: the stored URL as the web holds it (a
    // path on the API), not the absolute one this client loads.
    let raw = a
        .url
        .as_deref()
        .filter(|u| !u.is_empty())
        .or(a.file_key.as_deref())
        .unwrap_or_default();
    let source = if raw.starts_with('/') || is_web_url(raw) || raw.is_empty() {
        raw.to_owned()
    } else {
        format!("/api/{raw}")
    };
    let identity = a
        .file_key
        .as_deref()
        .filter(|k| !k.is_empty())
        .unwrap_or(&source);
    let size = a.size_bytes.map(|b| b as f64);
    let input = gif_favorite::FavoriteInput {
        id: Some(a.id.as_deref().filter(|i| !i.is_empty()).unwrap_or(&source)),
        title: Some(&a.filename),
        source_url: &source,
        preview_url: Some(&source),
        send_url: Some(&source),
        size_bytes: size,
        content_type: Some(mime),
        filename: Some(&a.filename),
        file_key_or_url: Some(identity),
        ..gif_favorite::FavoriteInput::default()
    };
    // `isAnimatedMedia(content_type, isGif, url || file_key)`.
    let animated = matches!(mime, "image/gif" | "image/apng")
        || gif_favorite::attachment_provider(Some(raw)).is_some()
        || gif_favorite::attachment_provider(Some(identity)).is_some()
        || is_gif_host(url);
    if animated {
        gif_favorite::attachment_gif(&input)
    } else if mime == "video/mp4" {
        gif_favorite::attachment_clip(&input)
    } else {
        Value::Null
    }
}

/// The message's attachments, in server order (images, videos and files
/// are bucketed by QML).
pub fn attachments_json(attachments: &[Attachment], api_base: &str) -> String {
    if attachments.is_empty() {
        return String::new();
    }
    let items: Vec<Value> = attachments
        .iter()
        .map(|a| {
            let mime = a.content_type.as_deref().unwrap_or_default();
            let url = attachment_url(a, api_base);
            // media.ts `isAnimatedMedia`: an animated type, a GIF host, or a
            // GIF provider's file (KLIPY, Tenor, an X GIF on fxtwitter, often
            // an animated WebP), so it plays like the web's <img>.
            let animated = matches!(mime, "image/gif" | "image/apng")
                || is_gif_host(&url)
                || gif_favorite::attachment_provider(a.file_key.as_deref()).is_some()
                || gif_favorite::attachment_provider(a.url.as_deref()).is_some();
            let bytes = a.size_bytes.unwrap_or(0);
            let kind = media_kind(mime);
            // ImageGrid.tsx and MessageItem.tsx: an animated picture or a
            // GIF/clip video carries the star.
            let favorite = if kind == "video" || kind == "image" {
                attachment_favorite(a, &url)
            } else {
                Value::Null
            };
            // GifProviderBranding.tsx: KLIPY's mark over its GIFs.
            let klipy = gif_favorite::attachment_provider(
                a.file_key
                    .as_deref()
                    .filter(|k| !k.is_empty())
                    .or(a.url.as_deref()),
            ) == Some("klipy");
            json!({
                "id": a.id.as_deref().unwrap_or_default(),
                "name": a.filename,
                "url": url,
                "kind": kind,
                "mime": mime,
                "size": if bytes > 0 { format_file_size(bytes) } else { String::new() },
                "gif": animated,
                "nsfw": a.is_nsfw,
                "favorite": favorite,
                "klipy": klipy,
            })
        })
        .collect();
    Value::Array(items).to_string()
}

/// `parseCustomEmojiToken`: `<:name:id>`.
fn custom_emoji(token: &str) -> Option<(&str, &str)> {
    let inner = token.strip_prefix("<:")?.strip_suffix('>')?;
    let (name, id) = inner.split_once(':')?;
    let ok = !name.is_empty()
        && !id.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    ok.then_some((name, id))
}

/// The image and `:label:` of a reaction or picker value: a native emoji,
/// `:shortcode:`, or a custom `<:name:id>` (served by the API).
pub fn emoji_image(value: &str, api_base: &str) -> (String, String) {
    if let Some((name, id)) = custom_emoji(value) {
        return (
            format!("{api_base}/api/emojis/assets/{id}"),
            format!(":{name}:"),
        );
    }
    match emoji::resolve(value) {
        Some(cell) => (cell.image_url(), format!(":{}:", cell.shortcode)),
        None => (String::new(), value.to_owned()),
    }
}

/// `formatReactionUserNames`: up to three names, then how many more.
pub fn reaction_names(names: &[&str], total: usize) -> String {
    let visible: Vec<&str> = names
        .iter()
        .map(|n| n.trim())
        .filter(|n| !n.is_empty())
        .take(3)
        .collect();
    if visible.is_empty() {
        return "Someone".to_owned();
    }
    let rest = total.saturating_sub(visible.len());
    let list = visible.join(", ");
    match rest {
        0 => list,
        1 => format!("{list} +1 person more"),
        n => format!("{list} +{n} people more"),
    }
}

/// The reaction strip: each chip's image, label, count, whether it is the
/// viewer's (the web reads `users`, not `me`) and its tooltip.
pub(super) fn reactions_json(reactions: &[Reaction], ctx: &RowContext<'_>) -> String {
    let chips: Vec<Value> = reactions
        .iter()
        .filter(|r| r.count > 0 && !r.emoji.is_empty())
        .map(|r| {
            let (url, label) = emoji_image(&r.emoji, ctx.api_base);
            let mine = if ctx.me_id.is_empty() {
                r.me
            } else {
                r.users.iter().any(|u| u == ctx.me_id)
            };
            let names: Vec<&str> = r
                .users
                .iter()
                .filter_map(|u| ctx.names.get(u).map(String::as_str))
                .collect();
            let total = usize::try_from(r.count).unwrap_or(0).max(r.users.len());
            let tip = format!("{} reacted with {label}", reaction_names(&names, total));
            json!({
                "emoji": r.emoji,
                "url": url,
                "label": label,
                "count": r.count,
                "mine": mine,
                "tip": tip,
            })
        })
        .collect();
    if chips.is_empty() {
        return String::new();
    }
    Value::Array(chips).to_string()
}

/// Hosts whose media only load through `/api/proxy-media` (X, TikTok,
/// Instagram; proxy-media.ts allowlist).
pub(crate) fn needs_proxy(url: &str) -> bool {
    host_of(url).is_some_and(|host| {
        matches!(
            host.as_str(),
            "pbs.twimg.com" | "video.twimg.com" | "gif.fxtwitter.com" | "vxtwitter.com"
        ) || host.ends_with(".cdninstagram.com")
            || host == "tiktok.com"
            || host.ends_with(".tiktok.com")
            || host.ends_with(".tiktokcdn.com")
            || host.ends_with(".tiktokcdn-us.com")
    })
}

/// An embed picture's URL: http(s) only, always through the media proxy
/// (X, TikTok and Instagram with their post, so the server can refresh them).
pub(super) fn embed_image_url(url: &str, page: &str, api_base: &str) -> String {
    if !is_web_url(url) {
        return String::new();
    }
    if needs_proxy(url) {
        return format!(
            "{api_base}/api/proxy-media?url={}&sourceUrl={}",
            query_value(url),
            query_value(page)
        );
    }
    via_media_proxy(url, api_base)
}

fn provider_is(embed: &Embed, names: &[&str], hosts: &[&str]) -> bool {
    let provider = embed
        .provider
        .as_ref()
        .map(|p| p.name.to_ascii_lowercase())
        .unwrap_or_default();
    let host = host_of(&embed.url).unwrap_or_default();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_owned();
    names.iter().any(|n| provider == *n)
        || hosts
            .iter()
            .any(|h| host == *h || host.ends_with(&format!(".{h}")))
}

/// `host + path` without `www.` (EmbedStatus.tsx's shortened link).
fn short_link(url: &str) -> String {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    rest.trim_end_matches('/').to_owned()
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|v| !v.is_empty())
}

/// One embed as QML draws it. Kinds: `image` and `video` (direct media,
/// bare), `status` (unavailable), `card` (everything else, players as a
/// thumbnail with a play disc that opens the link).
fn embed_json(embed: &Embed, api_base: &str) -> Option<Value> {
    if !is_web_url(&embed.url) {
        return None;
    }
    let youtube = provider_is(embed, &["youtube"], &["youtube.com", "youtu.be"]);
    let spotify = provider_is(embed, &["spotify"], &["spotify.com"]);
    let x = provider_is(
        embed,
        &["x", "twitter"],
        &["x.com", "twitter.com", "fxtwitter.com", "fixupx.com"],
    ) || embed.footer.as_ref().is_some_and(|f| f.text == "x");
    let provider = non_empty(embed.provider.as_ref().map(|p| p.name.as_str()))
        .map(str::to_owned)
        .unwrap_or_else(|| if x { "X".to_owned() } else { String::new() });
    if embed.unavailable {
        let noun = if youtube {
            "video"
        } else if spotify {
            "track"
        } else {
            "post"
        };
        let name = if provider.is_empty() {
            "Link"
        } else {
            &provider
        };
        return Some(json!({
            "kind": "status",
            "provider": name,
            "label": "Unavailable",
            "text": format!("This {noun} was deleted or made private."),
            "url": embed.url,
            "short": short_link(&embed.url),
            "open": format!("Open in {name}"),
        }));
    }
    let first_image = embed
        .media
        .iter()
        .find(|m| m.kind == "image" && is_web_url(&m.url));
    if embed.direct {
        if let Some(video) = embed.video.as_ref().filter(|v| is_web_url(&v.url)) {
            let name = video
                .url
                .split(['?', '#'])
                .next()
                .and_then(|p| p.rsplit('/').next())
                .unwrap_or_default();
            return Some(json!({
                "kind": "video",
                "url": video.url,
                "name": name,
                "host": short_link(&video.url).split('/').next().unwrap_or_default(),
            }));
        }
        let (url, width, height, gif) = match first_image {
            Some(m) => (m.url.as_str(), m.width, m.height, m.is_gif),
            None => {
                let t = embed.thumbnail.as_ref()?;
                (t.url.as_str(), t.width, t.height, false)
            }
        };
        let src = embed_image_url(url, &embed.url, api_base);
        return (!src.is_empty()).then(|| {
            json!({
                "kind": "image",
                "url": src,
                "w": width.unwrap_or(0.0),
                "h": height.unwrap_or(0.0),
                "gif": gif,
            })
        });
    }
    let player =
        embed.video.is_some() && (youtube || embed.kind == "video" || embed.kind == "rich");
    let picture = match (x, first_image, &embed.thumbnail) {
        (true, Some(m), _) => Some((m.url.as_str(), m.width, m.height)),
        (_, _, Some(t)) => Some((t.url.as_str(), t.width, t.height)),
        (_, Some(m), None) => Some((m.url.as_str(), m.width, m.height)),
        _ => None,
    };
    let (image, image_w, image_h) = picture
        .map(|(url, w, h)| (embed_image_url(url, &embed.url, api_base), w, h))
        .unwrap_or_default();
    let layout = if image.is_empty() {
        ""
    } else if player || x {
        "large"
    } else {
        match embed.image_layout.as_deref() {
            Some("thumbnail") => "thumbnail",
            _ => "large",
        }
    };
    let portrait = matches!((image_w, image_h), (Some(w), Some(h)) if h > w);
    let width = if youtube && player {
        if portrait {
            400
        } else {
            640
        }
    } else if layout == "large" {
        520
    } else {
        400
    };
    let video_box = embed.video.as_ref().map(|v| (v.width, v.height));
    let (box_w, box_h) = match video_box {
        Some((Some(w), Some(h))) if w > 0.0 && h > 0.0 => (w, h),
        _ => (image_w.unwrap_or(0.0), image_h.unwrap_or(0.0)),
    };
    let open = if youtube {
        "Open in YouTube".to_owned()
    } else if x {
        "Open post on X".to_owned()
    } else {
        "Open in a new tab".to_owned()
    };
    let author = embed
        .author
        .as_ref()
        .map(|a| a.name.trim().to_owned())
        .unwrap_or_default();
    Some(json!({
        "kind": "card",
        "url": embed.url,
        "provider": provider,
        "color": embed.color.as_deref().filter(|c| is_hex_colour(c)).unwrap_or_default(),
        "title": non_empty(embed.raw_title.as_deref()).unwrap_or_default(),
        "description": non_empty(embed.raw_description.as_deref()).unwrap_or_default(),
        "author": author,
        "image": image,
        "w": box_w,
        "h": box_h,
        "layout": layout,
        "play": player && !image.is_empty(),
        "width": width,
        "footer": embed.footer.as_ref().map(|f| f.text.trim()).filter(|t| !t.is_empty() && *t != "x").unwrap_or_default(),
        "open": open,
    }))
}

fn is_hex_colour(value: &str) -> bool {
    value
        .strip_prefix('#')
        .is_some_and(|hex| matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Every embed the message carries, in order (MessageItem.tsx draws all):
/// providers and media as [`super::embeds`] shapes them, link cards here.
pub fn embeds_json(
    embeds: &[Embed],
    api_base: &str,
    hydrations: &super::super::embeds::Hydrations,
    message_id: &str,
) -> String {
    let items: Vec<Value> = embeds
        .iter()
        .map(|e| unwrap_social_embed(e, api_base))
        .filter_map(|e| {
            super::embeds::embed_value(&e, api_base, hydrations, message_id)
                .or_else(|| embed_json(&e, api_base))
        })
        .collect();
    if items.is_empty() {
        return String::new();
    }
    Value::Array(items).to_string()
}

/// The original inside the server's social proxy form (our API, `url` and
/// `sourceUrl`, no signature): the X, TikTok and Instagram views read hosts
/// and file names, and build that same form again when they load the URL.
/// Signed generic URLs stay as delivered (they load without the session).
fn unwrap_social(url: &str, api_base: &str) -> String {
    let ours = url
        .strip_prefix(api_base.trim_end_matches('/'))
        .is_some_and(|rest| rest.starts_with("/api/proxy-media?"));
    if ours && url.contains("sourceUrl=") && !url.contains("&sig=") {
        return crate::gif_favorite::unwrap_proxy_media_url(url);
    }
    url.to_owned()
}

fn unwrap_social_embed(embed: &Embed, api_base: &str) -> Embed {
    let fix = |url: &str| unwrap_social(url, api_base);
    let fix_opt = |url: &Option<String>| url.as_deref().map(fix);
    let mut out = embed.clone();
    if let Some(author) = out.author.as_mut() {
        author.icon_url = fix_opt(&author.icon_url);
    }
    if let Some(thumbnail) = out.thumbnail.as_mut() {
        thumbnail.url = fix(&thumbnail.url);
    }
    for media in out.media.iter_mut() {
        media.url = fix(&media.url);
        media.thumbnail_url = fix_opt(&media.thumbnail_url);
    }
    if let Some(video) = out.video.as_mut() {
        video.url = fix(&video.url);
    }
    if let Some(audio) = out.audio.as_mut() {
        audio.url = fix_opt(&audio.url);
        audio.artwork_url = fix_opt(&audio.artwork_url);
    }
    if let Some(quoted) = out.referenced_tweet.as_mut() {
        if let Some(author) = quoted.author.as_mut() {
            author.icon_url = fix_opt(&author.icon_url);
        }
        for media in quoted.media.iter_mut() {
            media.url = fix(&media.url);
            media.thumbnail_url = fix_opt(&media.thumbnail_url);
        }
    }
    if let Some(footer) = out.footer.as_mut() {
        footer.icon_url = fix_opt(&footer.icon_url);
    }
    out
}

/// A local path as a `file:` URL QML can load.
pub fn file_url(path: &str) -> String {
    let path = path.replace('\\', "/");
    let encoded = encode_path(&path);
    if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}

/// The open channel's tray: finished files first, then the ones going
/// (AttachmentList.tsx order).
/// The open channel's tray: finished tiles first. `limit` is the user's
/// attachment limit, which the too-big cards name. A chunked upload going
/// shows its bytes sent of the total in the size chip ("54/150 MB",
/// AttachmentList.tsx `ProgressChip`).
pub(in crate::store) fn upload_rows(
    uploads: &[Upload],
    channel_id: Option<&str>,
    limit: u64,
) -> Vec<UploadRow> {
    let here = |u: &&Upload| Some(u.channel_id.as_str()) == channel_id;
    let done = uploads
        .iter()
        .filter(here)
        .filter(|u| matches!(u.status, UploadStatus::Done(_)));
    let rest = uploads
        .iter()
        .filter(here)
        .filter(|u| !matches!(u.status, UploadStatus::Done(_)));
    done.chain(rest)
        .map(|u| {
            let kind = media_kind(&u.content_type);
            let (state, progress, error) = match &u.status {
                UploadStatus::Uploading(sent) => {
                    let total = u.size.max(1);
                    let per_mille = sent.saturating_mul(1000) / total;
                    (
                        "uploading",
                        i64::try_from(per_mille.min(1000)).unwrap_or(0),
                        "",
                    )
                }
                UploadStatus::Done(_) => ("done", 1000, ""),
                UploadStatus::Failed(error) => ("failed", 0, error.as_str()),
                UploadStatus::TooBig => ("toobig", 0, ""),
                UploadStatus::Refused(_) => ("refused", 0, ""),
            };
            let card = match &u.status {
                UploadStatus::TooBig => {
                    crate::limits::tray_card(&u.name, u.size, limit).to_string()
                }
                UploadStatus::Refused(message) => {
                    crate::limits::server_card(message, limit, "remove").to_string()
                }
                _ => String::new(),
            };
            UploadRow {
                id: u.local_id.clone(),
                name: u.name.clone(),
                size: match u.status {
                    UploadStatus::Uploading(sent) if crate::limits::uses_multipart(u.size) => {
                        crate::limits::format_progress(sent, u.size)
                    }
                    _ => format_file_size(u.size),
                },
                kind: kind.to_owned(),
                preview: if kind == "image" {
                    file_url(&u.path)
                } else {
                    String::new()
                },
                state: state.to_owned(),
                progress,
                error: error.to_owned(),
                nsfw: u.nsfw,
                card,
            }
        })
        .collect()
}

/// The sent GIF stands in for a video preview only up to this size: KLIPY's
/// full GIFs run to tens of MB.
const MAX_SEND_AS_PREVIEW_BYTES: u64 = 1_000_000;

/// The picture a GIF tile shows (Qt has no player here): the server's
/// `thumb`, else an image preview, else the sent image when it is known to
/// be light or there is no video preview to loop instead.
pub fn gif_preview(item: &GifItem) -> Option<&str> {
    gif_preview_asset(item).map(|asset| asset.url.as_str())
}

fn gif_preview_asset(item: &GifItem) -> Option<&yap_protocol::rest::GifAsset> {
    let image = |asset: &yap_protocol::rest::GifAsset| {
        asset.content_type.starts_with("image/") && is_web_url(&asset.url)
    };
    let video_preview = gif_favorite::normalize_content_type(Some(&item.preview.content_type))
        == "video/mp4"
        && is_web_url(&item.preview.url);
    if let Some(thumb) = item.thumb.as_ref().filter(|t| image(t)) {
        Some(thumb)
    } else if image(&item.preview) {
        Some(&item.preview)
    } else if image(&item.send)
        && (!video_preview
            || (item.send.size_bytes > 0 && item.send.size_bytes <= MAX_SEND_AS_PREVIEW_BYTES))
    {
        Some(&item.send)
    } else {
        None
    }
}

/// A tile's video: a clip's mp4 (GifPickerModal.tsx `ClipVideoPlayer` plays
/// `gif.send`), else a video preview with no picture to fall back to
/// (`GifPreviewMedia` loops it muted).
fn gif_video<'a>(item: &'a GifItem, clip: bool) -> Option<&'a yap_protocol::rest::GifAsset> {
    let video = |asset: &'a yap_protocol::rest::GifAsset| {
        (gif_favorite::normalize_content_type(Some(&asset.content_type)) == "video/mp4"
            && is_web_url(&asset.url))
        .then_some(asset)
    };
    if clip {
        video(&item.send).or_else(|| video(&item.preview))
    } else if gif_preview(item).is_none() {
        video(&item.preview).or_else(|| video(&item.send))
    } else {
        None
    }
}

/// `formatDuration`: `m:ss`.
pub fn gif_duration(seconds: Option<f64>) -> String {
    match seconds {
        Some(s) if s.is_finite() && s >= 0.0 => {
            let s = s.floor() as i64;
            format!("{}:{:02}", s / 60, s % 60)
        }
        _ => String::new(),
    }
}

/// A shortcut tile's media (`GifShortcutCardMedia`): a picture, or a
/// muted loop when the item has only video.
fn shortcut_media(item: Option<&GifItem>) -> Value {
    let Some(item) = item else {
        return json!({ "image": "", "video": "" });
    };
    let image = gif_preview(item).unwrap_or_default();
    let video = gif_video(item, false)
        .map(|a| a.url.as_str())
        .unwrap_or_default();
    json!({ "image": image, "video": video })
}

/// The two-column masonry, filled round-robin like the web's grid, and
/// the rest of the picker: its start screen (categories, the Favorites and
/// Trending tiles' media), the provider and the search terms, as JSON.
/// `saved` is every favourite's `provider:id` (the tiles' stars) and
/// `favorite` the newest favourite of the media type.
pub(in crate::store) fn gif_rows(
    browse: &GifBrowse,
    saved: &[String],
    favorite: Option<&GifItem>,
) -> (Vec<GifRow>, GifStatusView) {
    let mut units = [0i64; 2];
    let mut counts = [0i64; 2];
    let mut rows = Vec::new();
    let clips = browse.media_type == "clips";
    let shown = browse.items.iter().filter_map(|item| {
        let video = gif_video(item, clips);
        let image = gif_preview_asset(item);
        (video.is_some() || image.is_some()).then_some((item, image, video))
    });
    for (i, (item, image, video)) in shown.enumerate() {
        let column = i % 2;
        let asset = video.or(image).unwrap_or(&item.send);
        let aspect = if asset.width > 0.0 && asset.height > 0.0 {
            // Clamped so one odd asset cannot make a tile a sliver or a tower.
            ((asset.height / asset.width).clamp(0.25, 2.5) * 1000.0).round() as i64
        } else {
            1000
        };
        let id = format!("{}:{}", item.provider, item.id);
        rows.push(GifRow {
            saved: saved.contains(&id),
            id,
            title: item
                .alt_text
                .as_deref()
                .filter(|t| !t.is_empty())
                .unwrap_or(&item.title)
                .to_owned(),
            preview: image.map(|a| a.url.clone()).unwrap_or_default(),
            video: video.map(|a| a.url.clone()).unwrap_or_default(),
            clip: clips && video.is_some(),
            duration: gif_duration(item.duration),
            aspect,
            column: column as i64,
            above: counts[column],
            top: units[column],
        });
        units[column] += aspect;
        counts[column] += 1;
    }
    let categories: Vec<Value> = browse
        .categories
        .get(&browse.start_key)
        .map(|list| {
            list.iter()
                .filter(|c| !c.label.is_empty())
                .map(|c| {
                    let image = if is_web_url(&c.image_url) {
                        c.image_url.as_str()
                    } else {
                        ""
                    };
                    json!({ "id": c.id, "label": c.label, "query": c.query, "image": image })
                })
                .collect()
        })
        .unwrap_or_default();
    let trending = browse
        .trending
        .get(&browse.start_key)
        .and_then(Option::as_ref);
    let mut terms: Vec<&str> = Vec::new();
    for term in browse.autocomplete.iter().chain(&browse.suggestions) {
        let term = term.trim();
        if !term.is_empty() && !terms.contains(&term) {
            terms.push(term);
        }
    }
    let browse_json = json!({
        "provider": browse.provider,
        "favorites": browse.favorites,
        "rateLimited": browse.rate_limited,
        "categoriesLoading": browse.categories_loading,
        "categories": categories,
        "trending": shortcut_media(trending),
        "favorite": shortcut_media(favorite),
        "terms": terms,
    });
    let status = GifStatusView {
        loading: browse.loading,
        more: browse.next.is_some(),
        error: browse.error.clone(),
        units,
        counts,
        browse: browse_json.to_string(),
    };
    (rows, status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_x_gif_webp_attachment_should_play_like_a_gif() {
        let a = Attachment {
            filename: "HKohayFWcAA3VCp.webp".into(),
            content_type: Some("image/webp".into()),
            url: Some("https://gif.fxtwitter.com/tweet_video/HKohayFWcAA3VCp.webp".into()),
            file_key: Some("https://gif.fxtwitter.com/tweet_video/HKohayFWcAA3VCp.webp".into()),
            ..Attachment::default()
        };
        let still = Attachment {
            content_type: Some("image/webp".into()),
            url: Some("https://cdn.example.com/photo.webp".into()),
            ..Attachment::default()
        };
        let json: Vec<serde_json::Value> =
            serde_json::from_str(&attachments_json(&[a, still], "https://yap.test")).expect("json");
        assert_eq!(json[0]["gif"], true);
        assert_eq!(json[1]["gif"], false);
    }

    #[test]
    fn file_sizes_should_format_like_the_web() {
        let sizes = [512, 1024, 1_258_291].map(format_file_size);
        assert_eq!(sizes, ["512 B", "1.0 KB", "1.2 MB"]);
    }

    #[test]
    fn relative_attachment_urls_should_be_absolute_and_encoded() {
        let a = Attachment {
            url: Some("/api/attachments/c/a/my file#1.png".into()),
            ..Attachment::default()
        };
        assert_eq!(
            attachment_url(&a, "https://yap.test"),
            "https://yap.test/api/attachments/c/a/my%20file%231.png"
        );
    }

    #[test]
    fn a_file_key_without_url_should_map_under_api() {
        let a = Attachment {
            file_key: Some("attachments/c/a/x.png".into()),
            ..Attachment::default()
        };
        assert_eq!(
            attachment_url(&a, "https://yap.test"),
            "https://yap.test/api/attachments/c/a/x.png"
        );
    }

    #[test]
    fn video_kinds_should_follow_the_playable_allowlist() {
        let kinds = [
            "video/mp4; codecs=x",
            "video/quicktime",
            "image/gif",
            "application/pdf",
        ]
        .map(media_kind);
        assert_eq!(kinds, ["video", "file", "image", "file"]);
    }

    #[test]
    fn reaction_names_should_cap_at_three() {
        assert_eq!(
            reaction_names(&["A", "B", "C", "D"], 5),
            "A, B, C +2 people more"
        );
    }

    #[test]
    fn reaction_names_without_any_should_say_someone() {
        assert_eq!(reaction_names(&[], 2), "Someone");
    }

    #[test]
    fn custom_emoji_should_load_from_the_api() {
        let (url, label) = emoji_image("<:yap_cat:abc-1>", "https://yap.test");
        assert_eq!(
            (url.as_str(), label.as_str()),
            ("https://yap.test/api/emojis/assets/abc-1", ":yap_cat:")
        );
    }

    #[test]
    fn x_media_should_go_through_the_proxy() {
        let url = embed_image_url(
            "https://pbs.twimg.com/a.jpg",
            "https://x.com/p",
            "https://y",
        );
        assert!(url.starts_with("https://y/api/proxy-media?url=https%3A%2F%2Fpbs.twimg.com"));
    }

    #[test]
    fn third_party_media_should_go_through_the_proxy() {
        assert_eq!(
            embed_image_url(
                "https://cdn.example/og.png?a=1",
                "https://news.example/s",
                "https://y"
            ),
            "https://y/api/proxy-media?url=https%3A%2F%2Fcdn.example%2Fog.png%3Fa%3D1"
        );
        assert_eq!(
            via_media_proxy("http://plain.example/a.gif", "https://y"),
            "https://y/api/proxy-media?url=http%3A%2F%2Fplain.example%2Fa.gif"
        );
    }

    #[test]
    fn our_own_and_server_built_proxy_urls_should_load_as_they_are() {
        for url in [
            "https://y/api/attachments/c/a/b.png",
            "https://y/api/proxy-media?url=https%3A%2F%2Fcdn.example%2Fa.png&sig=s",
            "file:///c:/x.png",
            "",
        ] {
            assert_eq!(via_media_proxy(url, "https://y"), url, "{url}");
        }
        // Another host's "proxy" path is never loaded from that host.
        assert_eq!(
            via_media_proxy(
                "https://evil.example/api/proxy-media?url=https%3A%2F%2Fcdn.example%2Fa.png&sig=s",
                "https://y"
            ),
            "https://y/api/proxy-media?url=https%3A%2F%2Fcdn.example%2Fa.png&sig=s"
        );
        assert_eq!(
            via_media_proxy("https://evil.example/api/proxy-media?id=1#x", "https://y"),
            "https://y/api/proxy-media?id=1"
        );
        assert!(is_proxy_url("https://y/api/proxy-media"));
        assert!(!is_proxy_url("https://y/api/proxy-media-not"));
        assert!(!is_proxy_url("https://evil.example/x?u=/api/proxy-media"));
    }

    #[test]
    fn the_social_proxy_form_should_unwrap_and_signed_urls_stay() {
        let social = "https://y/api/proxy-media?url=https%3A%2F%2Fvideo.twimg.com%2Ftweet_video%2Fa.mp4&sourceUrl=https%3A%2F%2Fx.com%2Fu%2Fstatus%2F1";
        assert_eq!(
            unwrap_social(social, "https://y"),
            "https://video.twimg.com/tweet_video/a.mp4"
        );
        let signed = "https://y/api/proxy-media?url=https%3A%2F%2Fcdn.example%2Fa.png&sig=s";
        assert_eq!(unwrap_social(signed, "https://y"), signed);
        let foreign = "https://evil.example/api/proxy-media?url=https%3A%2F%2Fa&sourceUrl=b";
        assert_eq!(unwrap_social(foreign, "https://y"), foreign);
    }

    #[test]
    fn embed_images_should_never_be_local_files() {
        assert_eq!(
            embed_image_url("file:///c:/x.png", "https://a", "https://y"),
            ""
        );
    }

    #[test]
    fn local_paths_should_become_file_urls() {
        assert_eq!(
            file_url(r"C:\Users\a b\x.png"),
            "file:///C:/Users/a%20b/x.png"
        );
    }
}
