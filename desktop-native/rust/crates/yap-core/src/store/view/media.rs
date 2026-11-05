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
use crate::store::state::{GifBrowse, Upload, UploadStatus};

/// The largest file the server takes (upload.ts, 25 MiB).
pub const MAX_UPLOAD_BYTES: u64 = 25 * 1024 * 1024;

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
    if raw.starts_with("file:") || raw.starts_with("qrc:") || is_web_url(raw) {
        return raw.to_owned();
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
            let animated = matches!(mime, "image/gif" | "image/apng") || is_gif_host(&url);
            let bytes = a.size_bytes.unwrap_or(0);
            json!({
                "id": a.id.as_deref().unwrap_or_default(),
                "name": a.filename,
                "url": url,
                "kind": media_kind(mime),
                "mime": mime,
                "size": if bytes > 0 { format_file_size(bytes) } else { String::new() },
                "gif": animated,
                "nsfw": a.is_nsfw,
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
fn needs_proxy(url: &str) -> bool {
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

/// An embed picture's URL: http(s) only, proxied where the web proxies it.
fn embed_image_url(url: &str, page: &str, api_base: &str) -> String {
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
    url.to_owned()
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

/// Every embed the message carries, in order (MessageItem.tsx draws all).
pub fn embeds_json(embeds: &[Embed], api_base: &str) -> String {
    let items: Vec<Value> = embeds
        .iter()
        .filter_map(|e| embed_json(e, api_base))
        .collect();
    if items.is_empty() {
        return String::new();
    }
    Value::Array(items).to_string()
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
pub(in crate::store) fn upload_rows(
    uploads: &[Upload],
    channel_id: Option<&str>,
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
            };
            UploadRow {
                id: u.local_id.clone(),
                name: u.name.clone(),
                size: format_file_size(u.size),
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
            }
        })
        .collect()
}

/// The URL a GIF tile shows: the preview, unless it is a video (Qt has no
/// player here), then the image that would be sent.
pub fn gif_preview(item: &GifItem) -> Option<&str> {
    let image = |asset: &yap_protocol::rest::GifAsset| {
        (asset.content_type.starts_with("image/") && is_web_url(&asset.url)).then_some(())
    };
    if image(&item.preview).is_some() {
        Some(item.preview.url.as_str())
    } else if image(&item.send).is_some() {
        Some(item.send.url.as_str())
    } else {
        None
    }
}

/// The two-column masonry, filled round-robin like the web's grid.
pub(in crate::store) fn gif_rows(browse: &GifBrowse) -> (Vec<GifRow>, GifStatusView) {
    let mut units = [0i64; 2];
    let mut counts = [0i64; 2];
    let mut rows = Vec::new();
    let shown = browse
        .items
        .iter()
        .filter_map(|item| Some((item, gif_preview(item)?)));
    for (i, (item, preview)) in shown.enumerate() {
        let column = i % 2;
        let asset = if item.preview.url == preview {
            &item.preview
        } else {
            &item.send
        };
        let aspect = if asset.width > 0.0 && asset.height > 0.0 {
            // Clamped so one odd asset cannot make a tile a sliver or a tower.
            ((asset.height / asset.width).clamp(0.25, 2.5) * 1000.0).round() as i64
        } else {
            1000
        };
        rows.push(GifRow {
            id: format!("{}:{}", item.provider, item.id),
            title: item
                .alt_text
                .as_deref()
                .filter(|t| !t.is_empty())
                .unwrap_or(&item.title)
                .to_owned(),
            preview: preview.to_owned(),
            aspect,
            column: column as i64,
            above: counts[column],
            top: units[column],
        });
        units[column] += aspect;
        counts[column] += 1;
    }
    let status = GifStatusView {
        loading: browse.loading,
        more: browse.next.is_some(),
        error: browse.error.clone(),
        units,
        counts,
    };
    (rows, status)
}

#[cfg(test)]
mod tests {
    use super::*;

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
