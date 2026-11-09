//! Link embeds as QML draws them (LinkEmbed.tsx's dispatch, :4947): one
//! JSON object per embed with a `kind` the card switches on.
//!
//! | kind      | web component      | plays                                    |
//! | --------- | ------------------ | ---------------------------------------- |
//! | `status`  | UnavailableSocial  | -                                        |
//! | `image`   | DirectMediaEmbed   | the viewer                               |
//! | `video`   | DirectMediaEmbed   | EmbedVideo (the file)                    |
//! | `youtube` | YouTubeEmbed       | WebEmbed (`player`)                      |
//! | `spotify` | SpotifyEmbed       | WebEmbed (`player`)                      |
//! | `social`  | TikTok/Instagram   | EmbedVideo (proxied), else WebEmbed      |
//! | `x`       | XEmbed             | EmbedVideo / AnimatedPicture (proxied)   |
//! | `videocard` | VideoEmbed       | opens the link                           |
//! | `rich`    | RichEmbed          | WebEmbed (`player`) when allowed         |
//! | `card`    | LinkEmbed_         | -                                        |
//!
//! TikTok and Instagram cards are refreshed through `/api/tiktok-video` and
//! `/api/instagram-video` (signed media expires); the answers live in the
//! store ([`Hydrations`]) and are merged here exactly as
//! `hydrateTikTokEmbed` / `hydrateInstagramEmbed` merge them.

use serde::Deserialize;
use serde_json::{json, Value};
use yap_protocol::rest::{
    Embed, EmbedAudio, EmbedAuthor, EmbedImage, EmbedMedia, EmbedMetrics, EmbedVideo,
};

use super::super::embeds::{Hydration, Hydrations, Provider};
use super::query_value;
use crate::gif_favorite;

const PORTRAIT_MAX: f64 = 560.0;

fn host_of(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?.split(':').next()?;
    Some(host.to_ascii_lowercase())
}

fn is_web(url: &str) -> bool {
    host_of(url).is_some_and(|h| !h.is_empty())
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|v| !v.is_empty())
}

/// `buildProxyMediaPath` made absolute: social media only loads through
/// the API's proxy (no auth; it allow-lists the CDNs and refreshes signed
/// URLs through `sourceUrl`).
pub fn proxied(url: &str, page: &str, api_base: &str) -> String {
    if !is_web(url) {
        return String::new();
    }
    // Only our own API's proxy URL passes through; a foreign URL that merely
    // carries the path is proxied like any other.
    if url.starts_with(&format!("{api_base}/api/proxy-media?")) {
        return url.to_owned();
    }
    format!(
        "{api_base}/api/proxy-media?url={}&sourceUrl={}",
        query_value(url),
        query_value(page)
    )
}

/// Social pictures that load straight (no signed CDN).
fn picture(url: &str, page: &str, api_base: &str) -> String {
    if !is_web(url) {
        return String::new();
    }
    if super::media::needs_proxy(url) {
        proxied(url, page, api_base)
    } else {
        url.to_owned()
    }
}

fn host_is(url: &str, domains: &[&str]) -> bool {
    let host = host_of(url).unwrap_or_default();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_owned();
    domains
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

fn provider_name(embed: &Embed) -> String {
    embed
        .provider
        .as_ref()
        .map(|p| p.name.trim().to_ascii_lowercase())
        .unwrap_or_default()
}

/// `tiktok.com/@a/video/1` (EmbedStatus.tsx `displayUrl`).
/// `decodeURIComponent` of a path segment (bad escapes stay as they are).
fn percent_decode(segment: &str) -> String {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| char::from(b).to_digit(16);
        match (
            bytes[i],
            bytes.get(i + 1).copied().and_then(hex),
            bytes.get(i + 2).copied().and_then(hex),
        ) {
            (b'%', Some(hi), Some(lo)) => {
                out.push(u8::try_from(hi * 16 + lo).unwrap_or(b'?'));
                i += 3;
            }
            (b, _, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| segment.to_owned())
}

fn display_url(url: &str) -> String {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    rest.trim_end_matches('/').to_owned()
}

fn noun(provider: &str) -> &'static str {
    match provider.to_ascii_lowercase().as_str() {
        "youtube" => "video",
        "spotify" => "track",
        _ => "post",
    }
}

/// EmbedStatus.tsx: `gone`, `restricted`, `failed` or `loading`.
fn status(
    state: &str,
    provider: &str,
    url: &str,
    transient: bool,
    author: Option<&EmbedAuthor>,
) -> Value {
    let noun = noun(provider);
    let (label, text) = match state {
        "restricted" => (
            "Restricted",
            format!("{provider} only shows this {noun} to signed-in viewers."),
        ),
        "failed" if transient => (
            "Didn't load",
            format!("{provider} didn't respond. Try again in a moment."),
        ),
        "failed" => (
            "Didn't load",
            format!("{provider} didn't send anything we can show."),
        ),
        "loading" => ("Loading", format!("Getting this {noun} from {provider}.")),
        _ => (
            "Unavailable",
            format!("This {noun} was deleted or made private."),
        ),
    };
    json!({
        "kind": "status",
        "state": if state.is_empty() { "gone" } else { state },
        "provider": provider,
        "label": label,
        "text": text,
        "url": url,
        "short": display_url(url),
        "open": format!("Open in {provider}"),
        "retry": state == "failed" && transient,
        "author": author.map(|a| a.name.trim()).unwrap_or_default(),
    })
}

/// `formatXMetricCount`: 1.2K, 3.4M, 1B.
pub fn metric_count(value: f64) -> String {
    let trim = |v: f64| {
        let text = if v >= 100.0 {
            format!("{v:.0}")
        } else {
            format!("{v:.1}")
        };
        text.strip_suffix(".0").map(str::to_owned).unwrap_or(text)
    };
    if value >= 1e9 {
        format!("{}B", trim(value / 1e9))
    } else if value >= 1e6 {
        format!("{}M", trim(value / 1e6))
    } else if value >= 1e3 {
        format!("{}K", trim(value / 1e3))
    } else {
        format!("{}", value.round() as i64)
    }
}

fn social_metric(value: Option<f64>) -> String {
    match value {
        Some(v) if v.is_finite() && v > 0.0 => metric_count(v),
        Some(v) if v.is_finite() && v == 0.0 => "0".to_owned(),
        _ => String::new(),
    }
}

fn thousands(value: f64) -> String {
    let digits = (value.round() as i64).to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn aspect_of(w: Option<f64>, h: Option<f64>) -> Option<f64> {
    match (w, h) {
        (Some(w), Some(h)) if w > 0.0 && h > 0.0 => Some(w / h),
        _ => None,
    }
}

// Hydration payloads (LinkEmbed.tsx `TikTokHydrationPayload`,
// `InstagramHydrationPayload`).

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct TikTokPayload {
    canonical_url: Option<String>,
    post_type: Option<String>,
    video_url: Option<String>,
    cover_url: Option<String>,
    title: Option<String>,
    author_name: Option<String>,
    author_handle: Option<String>,
    author_avatar_url: Option<String>,
    media: Vec<EmbedMedia>,
    audio: Option<EmbedAudio>,
    like_count: Option<f64>,
    comment_count: Option<f64>,
    view_count: Option<f64>,
    timestamp: Option<String>,
    restricted: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct InstagramPayload {
    video_url: Option<String>,
    thumbnail_url: Option<String>,
    title: Option<String>,
    duration_seconds: Option<f64>,
    media: Vec<EmbedMedia>,
    author_avatar_url: Option<String>,
    author_verified: Option<bool>,
    like_count: Option<f64>,
    comment_count: Option<f64>,
    view_count: Option<f64>,
    timestamp: Option<String>,
    audio: Option<EmbedAudio>,
}

fn direct_video_media(embed: &Embed, allow_player: bool) -> Option<EmbedMedia> {
    let video = embed.video.as_ref().filter(|v| !v.url.is_empty())?;
    let direct = video.kind.as_deref() == Some("direct");
    let usable = if allow_player {
        video.kind.as_deref() != Some("player")
    } else {
        direct
    };
    usable.then(|| EmbedMedia {
        kind: "video".into(),
        url: video.url.clone(),
        width: video.width,
        height: video.height,
        thumbnail_url: embed.thumbnail.as_ref().map(|t| t.url.clone()),
        content_type: video.content_type.clone(),
        is_gif: false,
        alt_text: None,
        duration_seconds: video.duration_seconds,
    })
}

fn tiktok_media(embed: &Embed) -> Vec<EmbedMedia> {
    if !embed.media.is_empty() {
        return embed.media.clone();
    }
    direct_video_media(embed, false).into_iter().collect()
}

fn instagram_media(embed: &Embed) -> Vec<EmbedMedia> {
    if !embed.media.is_empty() {
        return embed.media.clone();
    }
    if let Some(video) = direct_video_media(embed, true) {
        return vec![video];
    }
    embed
        .thumbnail
        .as_ref()
        .filter(|t| !t.url.is_empty())
        .map(|t| EmbedMedia {
            kind: "image".into(),
            url: t.url.clone(),
            width: t.width,
            height: t.height,
            ..EmbedMedia::default()
        })
        .into_iter()
        .collect()
}

fn merge_metrics(
    embed: &Embed,
    likes: Option<f64>,
    comments: Option<f64>,
    views: Option<f64>,
    prefer_payload: bool,
) -> EmbedMetrics {
    let mut metrics = embed.metrics.clone().unwrap_or_default();
    let pick = |old: Option<f64>, new: Option<f64>| {
        if prefer_payload {
            new.or(old)
        } else {
            old.or(new)
        }
    };
    metrics.likes = pick(metrics.likes, likes);
    metrics.comments = pick(metrics.comments, comments);
    metrics.views = pick(metrics.views, views);
    metrics
}

/// `hydrateTikTokEmbed`.
fn hydrate_tiktok(embed: &Embed, payload: &Value) -> Embed {
    let Ok(p) = serde_json::from_value::<TikTokPayload>(payload.clone()) else {
        return embed.clone();
    };
    let mut out = embed.clone();
    let has_video = p.video_url.is_some() || p.media.iter().any(|m| m.kind == "video");
    let has_slides =
        p.post_type.as_deref() == Some("slideshow") && p.media.iter().any(|m| m.kind == "image");
    let weak = !has_video && !has_slides;
    let embed_direct = embed.media.iter().any(|m| m.kind == "video")
        || embed
            .video
            .as_ref()
            .is_some_and(|v| !v.url.is_empty() && v.kind.as_deref() == Some("direct"));
    if weak && embed_direct {
        if let (Some(author), Some(icon)) = (
            out.author.as_mut(),
            non_empty(p.author_avatar_url.as_deref()),
        ) {
            author.icon_url = Some(icon.to_owned());
        }
        out.timestamp = out.timestamp.or(p.timestamp);
        out.audio = p.audio.or(out.audio);
        return out;
    }
    let media = if p.media.is_empty() {
        embed.media.clone()
    } else {
        p.media.clone()
    };
    let first_video = media.iter().find(|m| m.kind == "video").cloned();
    let first = media.first().cloned();
    if let Some(url) = non_empty(p.canonical_url.as_deref()) {
        out.url = url.to_owned();
    }
    let thumb_url = if p.post_type.as_deref() == Some("slideshow") {
        first_video
            .as_ref()
            .and_then(|v| v.thumbnail_url.clone())
            .or(p.cover_url.clone())
            .or(embed.thumbnail.as_ref().map(|t| t.url.clone()))
    } else {
        p.cover_url
            .clone()
            .or(embed.thumbnail.as_ref().map(|t| t.url.clone()))
            .or(first_video.as_ref().and_then(|v| v.thumbnail_url.clone()))
            .or(first.as_ref().map(|m| m.url.clone()))
    };
    if let Some(url) = thumb_url {
        let from_first = |f: fn(&EmbedMedia) -> Option<f64>| {
            first
                .as_ref()
                .filter(|m| m.kind == "image")
                .and_then(f)
                .or_else(|| first_video.as_ref().and_then(f))
        };
        out.thumbnail = Some(EmbedImage {
            url,
            width: embed
                .thumbnail
                .as_ref()
                .and_then(|t| t.width)
                .or_else(|| from_first(|m| m.width)),
            height: embed
                .thumbnail
                .as_ref()
                .and_then(|t| t.height)
                .or_else(|| from_first(|m| m.height)),
        });
    }
    let author_url = embed
        .author
        .as_ref()
        .and_then(|a| a.url.clone())
        .or_else(|| {
            p.author_handle
                .as_ref()
                .map(|h| format!("https://www.tiktok.com/@{h}"))
        });
    let author_name = embed
        .author
        .as_ref()
        .map(|a| a.name.clone())
        .filter(|n| !n.is_empty())
        .or(p.author_name.clone());
    if let Some(name) = author_name {
        out.author = Some(EmbedAuthor {
            name,
            url: author_url,
            icon_url: non_empty(p.author_avatar_url.as_deref())
                .map(str::to_owned)
                .or_else(|| embed.author.as_ref().and_then(|a| a.icon_url.clone())),
            is_verified: None,
        });
    }
    out.raw_title = embed.raw_title.clone().or(p.title.clone());
    out.raw_description = embed.raw_description.clone().or(p.title.clone());
    if let Some(video_url) = p.video_url.clone() {
        out.video = Some(EmbedVideo {
            url: video_url,
            width: first_video
                .as_ref()
                .and_then(|v| v.width)
                .or(embed.video.as_ref().and_then(|v| v.width))
                .or(Some(720.0)),
            height: first_video
                .as_ref()
                .and_then(|v| v.height)
                .or(embed.video.as_ref().and_then(|v| v.height))
                .or(Some(1280.0)),
            kind: Some("direct".into()),
            content_type: embed
                .video
                .as_ref()
                .and_then(|v| v.content_type.clone())
                .or(Some("video/mp4".into())),
            duration_seconds: embed
                .video
                .as_ref()
                .and_then(|v| v.duration_seconds)
                .or(first_video.as_ref().and_then(|v| v.duration_seconds)),
        });
    }
    out.media = media;
    out.metrics = Some(merge_metrics(
        embed,
        p.like_count,
        p.comment_count,
        p.view_count,
        false,
    ));
    out.timestamp = embed.timestamp.clone().or(p.timestamp);
    out.audio = p.audio.or(embed.audio.clone());
    out.restricted = p.restricted.unwrap_or(embed.restricted);
    out
}

/// `hydrateInstagramEmbed`.
fn hydrate_instagram(embed: &Embed, payload: &Value) -> Embed {
    let Ok(p) = serde_json::from_value::<InstagramPayload>(payload.clone()) else {
        return embed.clone();
    };
    let mut out = embed.clone();
    let weak =
        p.video_url.is_none() && !p.media.iter().any(|m| m.kind == "video") && p.media.len() <= 1;
    let embed_direct = embed.media.len() > 1
        || embed.media.iter().any(|m| m.kind == "video")
        || embed
            .video
            .as_ref()
            .is_some_and(|v| !v.url.is_empty() && v.kind.as_deref() != Some("player"));
    out.raw_title = p.title.clone().or(embed.raw_title.clone());
    if let Some(author) = out.author.as_mut() {
        if let Some(icon) = p.author_avatar_url.clone() {
            author.icon_url = Some(icon);
        }
        if p.author_verified.is_some() {
            author.is_verified = p.author_verified;
        }
    }
    out.metrics = Some(merge_metrics(
        embed,
        p.like_count,
        p.comment_count,
        p.view_count,
        true,
    ));
    out.timestamp = p.timestamp.clone().or(embed.timestamp.clone());
    out.audio = p.audio.clone().or(embed.audio.clone());
    if weak && embed_direct {
        return out;
    }
    let media = if p.media.is_empty() {
        embed.media.clone()
    } else {
        p.media.clone()
    };
    let first_video = media.iter().find(|m| m.kind == "video").cloned();
    let first = media.first().cloned();
    let thumb = p
        .thumbnail_url
        .clone()
        .or(first_video.as_ref().and_then(|v| v.thumbnail_url.clone()))
        .or(first
            .as_ref()
            .filter(|m| m.kind == "image")
            .map(|m| m.url.clone()))
        .or(embed.thumbnail.as_ref().map(|t| t.url.clone()));
    if let Some(url) = thumb {
        let image = first.as_ref().filter(|m| m.kind == "image");
        out.thumbnail = Some(EmbedImage {
            url,
            width: image
                .and_then(|m| m.width)
                .or(first_video.as_ref().and_then(|v| v.width))
                .or(embed.thumbnail.as_ref().and_then(|t| t.width)),
            height: image
                .and_then(|m| m.height)
                .or(first_video.as_ref().and_then(|v| v.height))
                .or(embed.thumbnail.as_ref().and_then(|t| t.height)),
        });
    }
    if let Some(video_url) = p.video_url.clone() {
        out.video = Some(EmbedVideo {
            url: video_url,
            width: first_video
                .as_ref()
                .and_then(|v| v.width)
                .or(embed.video.as_ref().and_then(|v| v.width))
                .or(Some(720.0)),
            height: first_video
                .as_ref()
                .and_then(|v| v.height)
                .or(embed.video.as_ref().and_then(|v| v.height))
                .or(Some(1280.0)),
            kind: Some("direct".into()),
            content_type: embed
                .video
                .as_ref()
                .and_then(|v| v.content_type.clone())
                .or(Some("video/mp4".into())),
            duration_seconds: p
                .duration_seconds
                .or(embed.video.as_ref().and_then(|v| v.duration_seconds)),
        });
    }
    out.media = media;
    out
}

// TikTok captions (LinkEmbed.tsx `getTikTokCaptionText`).

fn fold(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn tiktok_handle(author: Option<&EmbedAuthor>) -> Option<String> {
    let url = author?.url.as_deref()?;
    let at = url.find("/@")?;
    let handle = url[at + 2..].split(['/', '?', '#']).next()?.trim();
    (!handle.is_empty()).then(|| format!("@{handle}"))
}

fn is_author(value: &str, embed: &Embed) -> bool {
    let handle = tiktok_handle(embed.author.as_ref());
    let candidates = [
        embed
            .author
            .as_ref()
            .map(|a| a.name.trim().to_owned())
            .filter(|n| !n.is_empty()),
        handle.clone(),
        handle.map(|h| h.trim_start_matches('@').to_owned()),
    ];
    let value = fold(value);
    candidates.iter().flatten().any(|c| fold(c) == value)
}

fn tiktok_caption_candidate(value: Option<&str>, embed: &Embed) -> Option<String> {
    let candidate = non_empty(value)?;
    if candidate.to_lowercase().starts_with("tiktok") {
        return None;
    }
    let tags: Vec<&str> = candidate
        .split_whitespace()
        .filter(|w| w.starts_with('#') && w.len() > 1)
        .collect();
    if !tags.is_empty() {
        let rest: String = candidate
            .split_whitespace()
            .filter(|w| !w.starts_with('#'))
            .collect::<Vec<_>>()
            .join(" ");
        let rest = rest
            .trim_matches(|c: char| "|,.;:/\\()[]{} ".contains(c))
            .to_owned();
        if rest.is_empty() || is_author(&rest, embed) {
            return Some(tags.join(" "));
        }
    }
    if is_author(candidate, embed) {
        return None;
    }
    Some(candidate.to_owned())
}

fn tiktok_video_id(url: &str) -> Option<String> {
    for marker in ["/video/", "/photo/", "/player/v1/"] {
        if let Some(at) = url.find(marker) {
            let id: String = url[at + marker.len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if !id.is_empty() {
                return Some(id);
            }
        }
    }
    None
}

fn with_player_options(url: &str) -> String {
    let sep = if url.contains('?') { '&' } else { '?' };
    format!("{url}{sep}description=1&music_info=1")
}

fn media_json(
    item: &EmbedMedia,
    page: &str,
    api_base: &str,
    fallback_poster: Option<&str>,
) -> Value {
    let video = social_media_is_video(item);
    let poster = item
        .thumbnail_url
        .as_deref()
        .or(fallback_poster)
        .map(|p| proxied(p, page, api_base))
        .unwrap_or_default();
    json!({
        "type": if video { "video" } else { "image" },
        "src": proxied(&item.url, page, api_base),
        "poster": poster,
        "w": item.width.unwrap_or(0.0),
        "h": item.height.unwrap_or(0.0),
        "duration": item.duration_seconds.unwrap_or(0.0),
        "gif": item.is_gif,
        "alt": item.alt_text.as_deref().unwrap_or_default(),
    })
}

/// `shouldRenderInstagramMediaAsVideo` (TikTok's are typed the same way).
fn social_media_is_video(item: &EmbedMedia) -> bool {
    if item.kind == "video" || item.is_gif {
        return true;
    }
    if item
        .content_type
        .as_deref()
        .is_some_and(|t| t.to_ascii_lowercase().starts_with("video/"))
    {
        return true;
    }
    let path = item
        .url
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    [".mp4", ".m4v", ".mov", ".webm"]
        .iter()
        .any(|e| path.ends_with(e))
}

fn audio_json(audio: Option<&EmbedAudio>, page: &str, api_base: &str) -> Value {
    let Some(audio) = audio else {
        return Value::Null;
    };
    let title = non_empty(audio.title.as_deref()).unwrap_or_default();
    let artist = non_empty(audio.artist.as_deref()).unwrap_or_default();
    if title.is_empty() && artist.is_empty() {
        return Value::Null;
    }
    json!({
        "title": title,
        "artist": artist,
        "art": audio.artwork_url.as_deref().map(|u| proxied(u, page, api_base)).unwrap_or_default(),
        "src": audio.url.as_deref().map(|u| proxied(u, page, api_base)).unwrap_or_default(),
    })
}

/// TikTok and Instagram: the portrait social card.
fn social_json(
    embed: &Embed,
    provider: Provider,
    hydration: Option<&Hydration>,
    api_base: &str,
) -> Value {
    let (name, open) = match provider {
        Provider::TikTok => ("TikTok", "Open in TikTok"),
        Provider::Instagram => ("Instagram", "Open in Instagram"),
    };
    if matches!(hydration, Some(Hydration::Missing)) {
        return status("gone", name, &embed.url, false, embed.author.as_ref());
    }
    let shown = match (provider, hydration) {
        (Provider::TikTok, Some(Hydration::Ready(payload))) => hydrate_tiktok(embed, payload),
        (Provider::Instagram, Some(Hydration::Ready(payload))) => hydrate_instagram(embed, payload),
        _ => embed.clone(),
    };
    let page = embed.url.as_str();
    let media = match provider {
        Provider::TikTok => tiktok_media(&shown),
        Provider::Instagram => instagram_media(&shown),
    };
    let poster = shown.thumbnail.as_ref().map(|t| t.url.as_str());
    let items: Vec<Value> = media
        .iter()
        .map(|m| media_json(m, page, api_base, poster))
        .collect();
    let first = media.first();
    let aspect = first
        .and_then(|m| aspect_of(m.width, m.height))
        .or_else(|| {
            shown
                .thumbnail
                .as_ref()
                .and_then(|t| aspect_of(t.width, t.height))
        })
        .unwrap_or(match provider {
            Provider::TikTok => 9.0 / 16.0,
            Provider::Instagram => 1.0,
        });
    let metrics = shown.metrics.clone().unwrap_or_default();
    let caption = match provider {
        Provider::TikTok => tiktok_caption_candidate(shown.raw_description.as_deref(), &shown)
            .or_else(|| tiktok_caption_candidate(shown.raw_title.as_deref(), &shown))
            .unwrap_or_default(),
        Provider::Instagram => non_empty(shown.raw_description.as_deref())
            .or(non_empty(shown.raw_title.as_deref()))
            .unwrap_or_default()
            .to_owned(),
    };
    let iframe = match provider {
        Provider::TikTok => match shown
            .video
            .as_ref()
            .filter(|v| v.kind.as_deref() == Some("player"))
        {
            Some(video) => with_player_options(&video.url),
            None => tiktok_video_id(&shown.url)
                .map(|id| with_player_options(&format!("https://www.tiktok.com/player/v1/{id}")))
                .unwrap_or_default(),
        },
        Provider::Instagram => String::new(),
    };
    let all_images = !media.is_empty() && media.iter().all(|m| !social_media_is_video(m));
    let has_video = media.iter().any(social_media_is_video);
    // The player's state (TikTokEmbed's `player.mode`).
    let state = match hydration {
        None | Some(Hydration::Pending) if media.is_empty() => "loading",
        None | Some(Hydration::Pending) => "idle",
        Some(Hydration::Failed { .. }) if all_images || has_video => "error",
        Some(Hydration::Failed { .. }) => "iframe",
        Some(Hydration::Ready(_)) if media.is_empty() => "iframe",
        _ => "ready",
    };
    let restricted = shown.restricted && media.is_empty();
    if restricted {
        return status("restricted", name, &shown.url, false, shown.author.as_ref());
    }
    let renderable = !media.is_empty() && (all_images || state != "iframe");
    if !renderable {
        let transient = matches!(hydration, Some(Hydration::Failed { transient: true }));
        if state == "loading" && shown.thumbnail.is_none() {
            return status("loading", name, &shown.url, false, shown.author.as_ref());
        }
        if (state == "iframe" || state == "ready") && iframe.is_empty() {
            return status("failed", name, &shown.url, transient, shown.author.as_ref());
        }
    }
    let author = shown.author.as_ref();
    json!({
        "kind": "social",
        "provider": name,
        // EmbedHeader's date (EmbedDate), formatted in the view.
        "timestamp": shown.timestamp.as_deref().unwrap_or_default(),
        "url": shown.url,
        "page": embed.url,
        "open": open,
        "color": shown.color.as_deref().unwrap_or_default(),
        "authorName": author.map(|a| a.name.trim()).unwrap_or_default(),
        "authorUrl": author.and_then(|a| a.url.as_deref()).unwrap_or_default(),
        "authorAvatar": author.and_then(|a| a.icon_url.as_deref()).map(|u| proxied(u, page, api_base)).unwrap_or_default(),
        "verified": author.and_then(|a| a.is_verified).unwrap_or(false),
        "media": items,
        "renderable": renderable,
        "aspect": aspect,
        "maxHeight": PORTRAIT_MAX,
        "cover": shown.thumbnail.as_ref().map(|t| proxied(&t.url, page, api_base)).unwrap_or_default(),
        "state": state,
        "iframe": iframe,
        "likes": social_metric(metrics.likes),
        "comments": social_metric(metrics.comments),
        "views": social_metric(metrics.views),
        "commentsLink": match (provider, metrics.comments) {
            (Provider::Instagram, Some(c)) if c > 0.0 => format!("View all {} comments", thousands(c)),
            _ => String::new(),
        },
        "caption": caption,
        "captionAuthor": if provider == Provider::Instagram { author.map(|a| a.name.trim()).unwrap_or_default() } else { "" },
        "audio": audio_json(shown.audio.as_ref(), page, api_base),
        "title": shown.raw_title.as_deref().unwrap_or(match provider { Provider::TikTok => "TikTok video", Provider::Instagram => "Instagram post" }),
    })
}

/// `getXAuthorParts`: "Name (@handle)", else the name and the URL's first
/// path segment.
fn x_author_parts(author: &EmbedAuthor) -> (String, String) {
    let name = author.name.trim();
    if let Some(open) = name.rfind(" (@") {
        if let Some(handle) = name[open + 3..].strip_suffix(')') {
            return (
                name[..open].trim().to_owned(),
                format!("@{}", handle.trim().trim_start_matches('@')),
            );
        }
    }
    let handle = author
        .url
        .as_deref()
        .and_then(|u| u.split("://").nth(1))
        .and_then(|rest| rest.split('/').nth(1))
        .filter(|h| !h.is_empty())
        .map(|h| format!("@{}", h.trim_start_matches('@')))
        .unwrap_or_default();
    (name.to_owned(), handle)
}

/// `getFxTwitterGifWebpUrl`: the animated WebP of an X GIF.
fn x_gif_webp(url: &str) -> Option<String> {
    let path = url.strip_prefix("https://video.twimg.com")?;
    let path = path.split(['?', '#']).next()?;
    (path.starts_with("/tweet_video/") && path.to_ascii_lowercase().ends_with(".mp4"))
        .then(|| format!("https://gif.fxtwitter.com{}.webp", &path[..path.len() - 4]))
}

/// The star an X GIF or video carries (XGifTile's
/// `createExternalGifFavorite`, X videos' `createAttachmentClipFavorite`),
/// with the web's ids (`x-media-<message>-<index>-<url>`, the proxy path).
fn x_favorite(m: &EmbedMedia, index: usize, page: &str, api_base: &str, message_id: &str) -> Value {
    let message = if message_id.is_empty() {
        "embed"
    } else {
        message_id
    };
    let proxy = proxied(&m.url, page, api_base);
    let path = proxy.strip_prefix(api_base).unwrap_or(&proxy).to_owned();
    if m.is_gif {
        let webp = x_gif_webp(&m.url);
        let send = webp.clone().unwrap_or_else(|| path.clone());
        let id = format!("x-media-{message}-{index}-{}", m.url);
        let filename = format!(
            "x-video-{}.{}",
            index + 1,
            if webp.is_some() { "webp" } else { "mp4" }
        );
        let content = if webp.is_some() {
            "image/webp"
        } else {
            m.content_type.as_deref().unwrap_or("video/mp4")
        };
        return gif_favorite::external(&gif_favorite::FavoriteInput {
            id: Some(&id),
            title: Some(&filename),
            alt_text: m.alt_text.as_deref(),
            source_url: &m.url,
            preview_url: Some(&send),
            send_url: Some(&send),
            width: m.width,
            height: m.height,
            content_type: Some(content),
            ..gif_favorite::FavoriteInput::default()
        });
    }
    let id = format!("x-media-{message}-{index}-{}", m.url);
    let filename = format!("x-video-{}.mp4", index + 1);
    gif_favorite::attachment_clip(&gif_favorite::FavoriteInput {
        id: Some(&id),
        title: Some(&filename),
        source_url: &path,
        preview_url: Some(&path),
        send_url: Some(&path),
        width: m.width,
        height: m.height,
        filename: Some(&filename),
        file_key_or_url: Some(&path),
        ..gif_favorite::FavoriteInput::default()
    })
}

fn x_media(
    media: &[EmbedMedia],
    page: &str,
    api_base: &str,
    message_id: Option<&str>,
) -> Vec<Value> {
    media
        .iter()
        .enumerate()
        .map(|(index, m)| {
            let video = m.kind == "video";
            let gif = video && m.is_gif;
            let kind = if gif {
                "gif"
            } else if video {
                "video"
            } else {
                "image"
            };
            json!({
                "type": kind,
                "src": proxied(&m.url, page, api_base),
                "webp": if gif { x_gif_webp(&m.url).map(|w| proxied(&w, page, api_base)).unwrap_or_default() } else { String::new() },
                "poster": m.thumbnail_url.as_deref().map(|p| proxied(p, page, api_base)).unwrap_or_default(),
                "w": m.width.unwrap_or(0.0),
                "h": m.height.unwrap_or(0.0),
                "duration": m.duration_seconds.unwrap_or(0.0),
                "alt": m.alt_text.as_deref().unwrap_or_default(),
                // Quoted posts carry no star (the web draws none there).
                "favorite": match message_id {
                    Some(id) if video => x_favorite(m, index, page, api_base, id),
                    _ => Value::Null,
                },
            })
        })
        .collect()
}

fn collapses(text: &str, lines: usize, chars: usize) -> bool {
    text.lines().count() > lines || text.chars().count() > chars
}

fn x_json(embed: &Embed, api_base: &str, message_id: &str) -> Value {
    let page = embed.url.as_str();
    let media: Vec<EmbedMedia> = if !embed.media.is_empty() {
        embed.media.clone()
    } else if let Some(video) = direct_video_media(embed, true) {
        vec![video]
    } else {
        embed
            .thumbnail
            .as_ref()
            .filter(|t| !t.url.is_empty())
            .map(|t| EmbedMedia {
                kind: "image".into(),
                url: t.url.clone(),
                width: t.width,
                height: t.height,
                ..EmbedMedia::default()
            })
            .into_iter()
            .collect()
    };
    let shown: Vec<EmbedMedia> = media.iter().take(4).cloned().collect();
    let author = embed.author.as_ref().map(|a| {
        let (display, handle) = x_author_parts(a);
        json!({
            "name": if display.is_empty() { a.name.clone() } else { display },
            "handle": handle,
            "avatar": a.icon_url.as_deref().map(|u| picture(u, page, api_base)).unwrap_or_default(),
            "url": a.url.as_deref().unwrap_or_default(),
        })
    });
    let text = embed.raw_description.as_deref().unwrap_or_default();
    let card = embed.external_card.as_ref();
    let card_domain = card.and_then(|c| {
        non_empty(c.domain.as_deref())
            .map(str::to_owned)
            .or_else(|| {
                c.url
                    .as_deref()
                    .and_then(host_of)
                    .map(|h| h.trim_start_matches("www.").to_owned())
            })
    });
    let chip = card
        .filter(|_| shown.len() == 1)
        .and_then(|c| {
            non_empty(c.title.as_deref())
                .map(str::to_owned)
                .or(card_domain.clone())
                .or(c.url.clone())
        })
        .unwrap_or_default();
    let quote = embed.referenced_tweet.as_ref().map(|q| {
        let author = q.author.as_ref().map(|a| {
            let (display, handle) = x_author_parts(a);
            json!({
                "name": if display.is_empty() { a.name.clone() } else { display },
                "handle": handle,
                "avatar": a.icon_url.as_deref().map(|u| picture(u, page, api_base)).unwrap_or_default(),
            })
        });
        let qtext = q.raw_description.as_deref().unwrap_or_default();
        let qpage = q.url.as_deref().unwrap_or(page);
        json!({
            "author": author,
            "text": qtext,
            "collapse": collapses(qtext, 4, 220),
            "url": q.url.as_deref().unwrap_or_default(),
            "timestamp": q.timestamp.as_deref().unwrap_or_default(),
            "media": x_media(&q.media.iter().take(4).cloned().collect::<Vec<_>>(), qpage, api_base, None),
            "extra": q.media.len().saturating_sub(4),
        })
    });
    let metrics = embed.metrics.clone().unwrap_or_default();
    let bar: Vec<Value> = [
        ("replies", "Replies", metrics.replies),
        ("retweets", "Reposts", metrics.retweets),
        ("likes", "Likes", metrics.likes),
        ("impressions", "Impressions", metrics.impressions),
    ]
    .iter()
    .filter_map(|(key, label, value)| {
        let value = (*value)?;
        Some(json!({
            "key": key,
            "label": label,
            "value": if value > 0.0 { metric_count(value) } else { String::new() },
            "raw": thousands(value),
        }))
    })
    .collect();
    json!({
        "kind": "x",
        "timestamp": embed.timestamp.as_deref().unwrap_or_default(),
        "url": embed.url,
        "provider": non_empty(embed.provider.as_ref().map(|p| p.name.as_str())).unwrap_or("X"),
        "color": embed.color.as_deref().unwrap_or_default(),
        "open": "Open post on X",
        "author": author,
        "text": text,
        "collapse": collapses(text, 5, 280),
        "media": x_media(&shown, page, api_base, Some(message_id)),
        "extra": media.len().saturating_sub(4),
        "chip": chip,
        "cardUrl": card.and_then(|c| c.url.as_deref()).filter(|_| !shown.is_empty() && card_domain.is_some()).unwrap_or_default(),
        "cardDomain": card_domain.filter(|_| !shown.is_empty()).unwrap_or_default(),
        "quote": quote,
        "metrics": bar,
    })
}

fn direct_json(embed: &Embed, api_base: &str) -> Option<Value> {
    let file_name = |url: &str, fallback: &str| {
        url.split(['?', '#'])
            .next()
            .and_then(|p| p.rsplit('/').find(|s| !s.is_empty()))
            .map(percent_decode)
            .filter(|s| !s.is_empty() && !s.contains("://"))
            .unwrap_or_else(|| fallback.to_owned())
    };
    let host = host_of(&embed.url)
        .unwrap_or_default()
        .trim_start_matches("www.")
        .to_owned();
    if let Some(video) = embed
        .video
        .as_ref()
        .filter(|v| v.kind.as_deref() == Some("direct") && is_web(&v.url))
    {
        return Some(json!({
            "kind": "video",
            "src": video.url,
            "url": embed.url,
            "w": video.width.unwrap_or(0.0),
            "h": video.height.unwrap_or(0.0),
            "duration": video.duration_seconds.unwrap_or(0.0),
            "name": file_name(&video.url, "video.mp4"),
            "host": host,
        }));
    }
    let image = embed
        .media
        .iter()
        .find(|m| m.kind == "image" && is_web(&m.url));
    match image {
        Some(m) => Some(json!({
            "kind": "image",
            "url": m.url,
            "w": m.width.unwrap_or(0.0),
            "h": m.height.unwrap_or(0.0),
            "gif": m.is_gif,
            "name": file_name(&m.url, "image"),
            "host": host,
        })),
        None => Some(json!({
            "kind": "fallback",
            "url": embed.url,
            "name": file_name(&embed.url, "Media"),
            "host": host,
        })),
    }
    .map(|mut v| {
        if v["kind"] == "image" {
            let src = super::media::embed_image_url(
                v["url"].as_str().unwrap_or_default(),
                &embed.url,
                api_base,
            );
            v["url"] = Value::String(src);
        }
        v
    })
}

/// `getSpotifyEmbedUrl` without the `intl-xx` segment, and the player's
/// height (152 for collections, else 80).
fn spotify(url: &str) -> Option<(String, i64)> {
    let rest = url.strip_prefix("https://open.spotify.com/")?;
    let path = rest.split(['?', '#']).next()?;
    let path = match path.strip_prefix("intl-") {
        Some(after) => after.split_once('/').map_or(path, |(_, p)| p),
        None => path,
    };
    let height = if ["album/", "playlist/", "artist/", "show/"]
        .iter()
        .any(|k| path.starts_with(k))
    {
        152
    } else {
        80
    };
    Some((
        format!("https://open.spotify.com/embed/{path}?utm_source=generator"),
        height,
    ))
}

fn youtube_json(embed: &Embed) -> Value {
    let video = embed.video.as_ref();
    let w = video
        .and_then(|v| v.width)
        .filter(|w| *w > 0.0)
        .unwrap_or(1280.0);
    let h = video
        .and_then(|v| v.height)
        .filter(|h| *h > 0.0)
        .unwrap_or(720.0);
    let portrait = h > w;
    let player = video
        .map(|v| format!("{}?autoplay=1&rel=0", v.url))
        .unwrap_or_default();
    json!({
        "kind": "youtube",
        "url": embed.url,
        "provider": non_empty(embed.provider.as_ref().map(|p| p.name.as_str())).unwrap_or("YouTube"),
        "color": embed.color.as_deref().unwrap_or_default(),
        "title": non_empty(embed.raw_title.as_deref()).unwrap_or_default(),
        "description": non_empty(embed.raw_description.as_deref()).unwrap_or_default(),
        "footer": embed.footer.as_ref().map(|f| f.text.trim()).unwrap_or_default(),
        "thumb": embed.thumbnail.as_ref().map(|t| t.url.as_str()).filter(|u| is_web(u)).unwrap_or_default(),
        "w": w,
        "h": h,
        "portrait": portrait,
        "width": if portrait { 400 } else { 640 },
        "player": player,
        "open": "Open in YouTube",
    })
}

/// RichEmbed and VideoEmbed share the link card's shape: the thumbnail
/// large under the text (at most 300 high; a video page's 400) with the
/// play disc. A rich embed's `player` plays in place when its host is
/// allowed; a video page's opens the link.
fn player_card_json(embed: &Embed, api_base: &str, rich: bool) -> Value {
    let thumb = embed.thumbnail.as_ref().filter(|t| is_web(&t.url));
    let image = thumb
        .map(|t| super::media::embed_image_url(&t.url, &embed.url, api_base))
        .unwrap_or_default();
    let player = embed
        .video
        .as_ref()
        .map(|v| v.url.clone())
        .filter(|u| rich && is_web(u))
        .unwrap_or_default();
    json!({
        "kind": "card",
        "url": embed.url,
        "provider": embed.provider.as_ref().map(|p| p.name.trim()).unwrap_or_default(),
        "color": embed.color.as_deref().unwrap_or_default(),
        "title": non_empty(embed.raw_title.as_deref()).unwrap_or_default(),
        "description": non_empty(embed.raw_description.as_deref()).unwrap_or_default(),
        "footer": embed.footer.as_ref().map(|f| f.text.trim()).unwrap_or_default(),
        "author": embed.author.as_ref().map(|a| a.name.trim()).unwrap_or_default(),
        "image": image,
        "w": thumb.and_then(|t| t.width).unwrap_or(0.0),
        "h": thumb.and_then(|t| t.height).unwrap_or(0.0),
        "layout": if image.is_empty() { "" } else { "large" },
        "width": 520,
        "play": !image.is_empty() && (!rich || embed.video.is_some()),
        "player": player,
        "maxH": if rich { 300 } else { 400 },
        "open": "Open in a new tab",
    })
}

/// The provider an embed is hydrated from, if any.
pub fn hydration_provider(embed: &Embed) -> Option<Provider> {
    if embed.unavailable || embed.direct {
        return None;
    }
    match provider_name(embed).as_str() {
        "tiktok" => Some(Provider::TikTok),
        "instagram" => Some(Provider::Instagram),
        _ => None,
    }
}

/// One embed for QML, or `None` for one that cannot show.
pub fn embed_value(
    embed: &Embed,
    api_base: &str,
    hydrations: &Hydrations,
    message_id: &str,
) -> Option<Value> {
    if !is_web(&embed.url) {
        return None;
    }
    let provider = provider_name(embed);
    if embed.unavailable {
        let name = embed
            .provider
            .as_ref()
            .map(|p| p.name.trim())
            .filter(|n| !n.is_empty())
            .unwrap_or("Link");
        return Some(status(
            "gone",
            name,
            &embed.url,
            false,
            embed.author.as_ref(),
        ));
    }
    if embed.direct {
        return direct_json(embed, api_base);
    }
    if provider == "youtube" && embed.video.as_ref().is_some_and(|v| !v.url.is_empty()) {
        return Some(youtube_json(embed));
    }
    if let Some(kind) = hydration_provider(embed) {
        let hydration = hydrations
            .get(&super::super::embeds::key(kind, &embed.url))
            .map(|e| &e.state);
        return Some(social_json(embed, kind, hydration, api_base));
    }
    if provider == "spotify" {
        if let Some((player, height)) = spotify(&embed.url) {
            return Some(json!({
                "kind": "spotify",
                "url": embed.url,
                "player": player,
                "height": height,
                "title": non_empty(embed.raw_title.as_deref()).unwrap_or("Spotify"),
                "description": non_empty(embed.raw_description.as_deref()).unwrap_or_default(),
                "thumb": embed.thumbnail.as_ref().map(|t| t.url.as_str()).filter(|u| is_web(u)).unwrap_or_default(),
            }));
        }
    }
    let x = provider == "x"
        || provider == "twitter"
        || embed
            .footer
            .as_ref()
            .is_some_and(|f| f.text.eq_ignore_ascii_case("x"))
        || host_is(
            &embed.url,
            &["x.com", "twitter.com", "fxtwitter.com", "fixupx.com"],
        );
    if x {
        return Some(x_json(embed, api_base, message_id));
    }
    match embed.kind.as_str() {
        "video" => Some(player_card_json(embed, api_base, false)),
        "rich" => Some(player_card_json(embed, api_base, true)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn embed(provider: &str, url: &str) -> Embed {
        Embed {
            url: url.into(),
            kind: "video".into(),
            provider: Some(yap_protocol::rest::EmbedProvider {
                name: provider.into(),
                url: None,
            }),
            ..Embed::default()
        }
    }

    #[test]
    fn our_own_proxy_urls_should_pass_through() {
        let own = "https://y/api/proxy-media?url=https%3A%2F%2Fa.example%2Fv.mp4";
        assert_eq!(proxied(own, "https://page", "https://y"), own);
    }

    #[test]
    fn a_foreign_proxy_lookalike_should_be_proxied_again() {
        let foreign = "https://evil.example/api/proxy-media?url=x";
        let out = proxied(foreign, "https://page", "https://y");
        assert!(
            out.starts_with("https://y/api/proxy-media?url=https%3A%2F%2Fevil.example"),
            "{out}"
        );
    }

    #[test]
    fn metric_counts_should_shorten_like_the_web() {
        let counts = [999.0, 1_200.0, 15_300.0, 2_000_000.0].map(metric_count);
        assert_eq!(counts, ["999", "1.2K", "15.3K", "2M"]);
    }

    #[test]
    fn x_gifs_should_get_their_animated_webp() {
        assert_eq!(
            x_gif_webp("https://video.twimg.com/tweet_video/Gabc.mp4").as_deref(),
            Some("https://gif.fxtwitter.com/tweet_video/Gabc.webp")
        );
    }

    #[test]
    fn x_authors_should_split_name_and_handle() {
        let author = EmbedAuthor {
            name: "Jo (@jo)".into(),
            ..EmbedAuthor::default()
        };
        assert_eq!(x_author_parts(&author), ("Jo".into(), "@jo".into()));
    }

    #[test]
    fn spotify_collections_should_get_the_tall_player() {
        assert_eq!(
            spotify("https://open.spotify.com/intl-de/album/x?si=1"),
            Some((
                "https://open.spotify.com/embed/album/x?utm_source=generator".into(),
                152
            ))
        );
    }

    #[test]
    fn tiktok_without_media_should_ask_while_loading() {
        let v = embed_value(
            &embed("TikTok", "https://www.tiktok.com/@a/video/123"),
            "https://y",
            &HashMap::new(),
            "m",
        )
        .unwrap_or_default();
        assert_eq!(v["state"], "loading");
    }

    #[test]
    fn tiktok_should_fall_back_to_its_player_when_the_lookup_fails() {
        let mut hydrations = HashMap::new();
        hydrations.insert(
            super::super::super::embeds::key(
                Provider::TikTok,
                "https://www.tiktok.com/@a/video/123",
            ),
            super::super::super::embeds::Entry {
                state: Hydration::Failed { transient: false },
                at_ms: 0,
            },
        );
        let mut e = embed("TikTok", "https://www.tiktok.com/@a/video/123");
        e.thumbnail = Some(EmbedImage {
            url: "https://p16.tiktokcdn.com/c.jpg".into(),
            width: None,
            height: None,
        });
        let v = embed_value(&e, "https://y", &hydrations, "m").unwrap_or_default();
        assert_eq!(
            v["iframe"],
            "https://www.tiktok.com/player/v1/123?description=1&music_info=1"
        );
    }

    #[test]
    fn a_hydrated_tiktok_should_play_its_proxied_video() {
        let mut hydrations = HashMap::new();
        hydrations.insert(
            super::super::super::embeds::key(Provider::TikTok, "https://www.tiktok.com/@a/video/1"),
            super::super::super::embeds::Entry {
                state: Hydration::Ready(json!({"videoUrl": "https://v16.tiktok.com/v.mp4", "media": [{"type": "video", "url": "https://v16.tiktok.com/v.mp4", "width": 576, "height": 1024}]})),
                at_ms: 0,
            },
        );
        let v = embed_value(
            &embed("TikTok", "https://www.tiktok.com/@a/video/1"),
            "https://y",
            &hydrations,
            "m",
        )
        .unwrap_or_default();
        assert!(v["media"][0]["src"]
            .as_str()
            .unwrap_or_default()
            .starts_with("https://y/api/proxy-media?url=https%3A%2F%2Fv16.tiktok.com"));
    }

    #[test]
    fn tiktok_captions_that_repeat_the_author_should_hide() {
        let mut e = embed("TikTok", "https://www.tiktok.com/@scout/video/1");
        e.author = Some(EmbedAuthor {
            name: "Scout".into(),
            url: Some("https://www.tiktok.com/@scout".into()),
            ..EmbedAuthor::default()
        });
        assert_eq!(tiktok_caption_candidate(Some("scout"), &e), None);
    }

    #[test]
    fn a_youtube_short_should_take_the_compact_width() {
        let mut e = embed("YouTube", "https://www.youtube.com/shorts/x");
        e.video = Some(EmbedVideo {
            url: "https://www.youtube.com/embed/x".into(),
            width: Some(720.0),
            height: Some(1280.0),
            ..EmbedVideo::default()
        });
        assert_eq!(youtube_json(&e)["width"], 400);
    }

    #[test]
    fn the_youtube_player_should_autoplay() {
        let mut e = embed("YouTube", "https://www.youtube.com/watch?v=x");
        e.video = Some(EmbedVideo {
            url: "https://www.youtube.com/embed/x".into(),
            ..EmbedVideo::default()
        });
        assert_eq!(
            youtube_json(&e)["player"],
            "https://www.youtube.com/embed/x?autoplay=1&rel=0"
        );
    }

    #[test]
    fn a_direct_video_should_play_the_file() {
        let mut e = embed("", "https://cdn.example/clip.mp4");
        e.direct = true;
        e.video = Some(EmbedVideo {
            url: "https://cdn.example/clip.mp4".into(),
            kind: Some("direct".into()),
            ..EmbedVideo::default()
        });
        let v = embed_value(&e, "https://y", &HashMap::new(), "m").unwrap_or_default();
        assert_eq!(
            (v["kind"].as_str(), v["name"].as_str()),
            (Some("video"), Some("clip.mp4"))
        );
    }
}
