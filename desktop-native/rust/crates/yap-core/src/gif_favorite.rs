//! The favourite a GIF or clip in a message saves (gif-favorite-item.ts,
//! gif-picker.ts): a `GifPickerItem` as the web builds it from an
//! attachment or an X GIF, and its identity (`provider:id`), which is how
//! the star knows it is already saved.

use reqwest::Url;
use serde_json::{json, Value};

/// What a favourite is made from (`AnimatedFavoriteInput` and
/// `AttachmentFavoriteInput`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FavoriteInput<'a> {
    pub id: Option<&'a str>,
    pub title: Option<&'a str>,
    pub alt_text: Option<&'a str>,
    pub source_url: &'a str,
    pub preview_url: Option<&'a str>,
    pub send_url: Option<&'a str>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub size_bytes: Option<f64>,
    pub content_type: Option<&'a str>,
    pub duration: Option<f64>,
    pub filename: Option<&'a str>,
    pub file_key_or_url: Option<&'a str>,
}

fn present(value: Option<&str>) -> Option<&str> {
    value.filter(|v| !v.is_empty())
}

/// `normalizeGifPickerContentType`.
pub fn normalize_content_type(value: Option<&str>) -> &'static str {
    let mime = value
        .unwrap_or_default()
        .to_ascii_lowercase()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned();
    match mime.as_str() {
        "image/apng" => "image/apng",
        "image/webp" => "image/webp",
        "image/png" => "image/png",
        m if m == "video/mp4" || m.starts_with("video/") => "video/mp4",
        _ => "image/gif",
    }
}

/// `unwrapProxyMediaUrl`: the `url` of an `/api/proxy-media` link.
pub fn unwrap_proxy_media_url(url: &str) -> String {
    let parsed = Url::parse(url).or_else(|_| Url::parse("https://localhost")?.join(url));
    let Ok(parsed) = parsed else {
        return url.to_owned();
    };
    if parsed.path().ends_with("/api/proxy-media") {
        if let Some((_, inner)) = parsed.query_pairs().find(|(k, _)| k == "url") {
            if !inner.is_empty() {
                return inner.into_owned();
            }
        }
    }
    url.to_owned()
}

/// `getGifAttachmentProvider`: `klipy`, `tenor`, `external` (an X GIF) or
/// none.
pub fn attachment_provider(file_key_or_url: Option<&str>) -> Option<&'static str> {
    let raw = present(file_key_or_url)?;
    let unwrapped = unwrap_proxy_media_url(raw);
    for text in [raw.to_ascii_lowercase(), unwrapped.to_ascii_lowercase()] {
        if text.contains("/gifs/klipy/") || text.contains("\\gifs\\klipy\\") {
            return Some("klipy");
        }
        if text.contains("/gifs/tenor/") || text.contains("\\gifs\\tenor\\") {
            return Some("tenor");
        }
    }
    let parsed = Url::parse(&unwrapped).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    let numbered = |prefix: &str, suffix: &str| {
        host.strip_prefix(prefix)
            .and_then(|rest| rest.strip_suffix(suffix))
            .is_some_and(|digits| digits.chars().all(|c| c.is_ascii_digit()))
    };
    if numbered("static", ".klipy.com") {
        return Some("klipy");
    }
    if host == "tenor.com" || numbered("media", ".tenor.com") {
        return Some("tenor");
    }
    if host == "gif.fxtwitter.com" && parsed.path().starts_with("/tweet_video/") {
        return Some("external");
    }
    None
}

/// `getXGifSourceUrl`: an X GIF's mp4 on video.twimg.com.
fn x_gif_source_url(source_url: Option<&str>) -> Option<String> {
    let unwrapped = unwrap_proxy_media_url(present(source_url)?);
    let parsed = Url::parse(&unwrapped).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    let path = parsed.path();
    let lower = path.to_ascii_lowercase();
    if host == "video.twimg.com" && path.starts_with("/tweet_video/") && lower.ends_with(".mp4") {
        return Some(format!("https://video.twimg.com{path}"));
    }
    if host == "gif.fxtwitter.com" && path.starts_with("/tweet_video/") && lower.ends_with(".webp")
    {
        let stem = &path[..path.len() - ".webp".len()];
        return Some(format!("https://video.twimg.com{stem}.mp4"));
    }
    None
}

/// `getFxTwitterGifWebpUrl`: the animated WebP gif.fxtwitter.com serves
/// for an X GIF.
pub fn fx_twitter_webp_url(source_url: &str) -> Option<String> {
    let mp4 = x_gif_source_url(Some(source_url))?;
    let path = Url::parse(&mp4).ok()?.path().to_owned();
    let stem = path.strip_suffix(".mp4").unwrap_or(&path);
    Some(format!("https://gif.fxtwitter.com{stem}.webp"))
}

/// `inferGifPickerMediaType`.
fn media_type(item: &Value) -> &'static str {
    let text = |v: &Value| v.as_str().unwrap_or_default().to_owned();
    let send = text(&item["send"]["url"]);
    let preview = text(&item["preview"]["url"]);
    let source = text(&item["sourceUrl"]);
    let content = normalize_content_type(item["send"]["contentType"].as_str());
    let title = text(&item["title"]).to_ascii_lowercase();
    let id = text(&item["id"]).to_ascii_lowercase();
    let has_duration = !item["duration"].is_null();
    if has_duration
        || content == "video/mp4"
        || send.contains(".mp4")
        || send.contains("/clips/")
        || id.contains("clip")
    {
        return "clips";
    }
    let meme = |v: &str| v.to_ascii_lowercase().contains("/static-memes/");
    if meme(&send) || meme(&preview) || meme(&source) {
        return "memes";
    }
    if content == "image/apng"
        || send.contains("/stickers/")
        || preview.contains("/stickers/")
        || send.contains("sticker")
        || preview.contains("sticker")
        || title.contains("sticker")
        || id.contains("sticker")
    {
        return "stickers";
    }
    "gifs"
}

fn positive(value: Option<f64>, fallback: f64) -> f64 {
    value
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(fallback)
}

fn filename_stem(filename: Option<&str>) -> Option<String> {
    let trimmed = filename?.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(match trimmed.rfind('.') {
        Some(dot) if dot > 0 => trimmed[..dot].to_owned(),
        _ => trimmed.to_owned(),
    })
}

/// `createExternalGifFavorite`.
pub fn external(input: &FavoriteInput<'_>) -> Value {
    let width = positive(input.width, 320.0);
    let height = positive(input.height, 320.0);
    let size_bytes = positive(input.size_bytes, 0.0).floor().max(0.0);
    let content_type = normalize_content_type(input.content_type);
    let send_url = present(input.send_url).unwrap_or(input.source_url);
    let preview_url = present(input.preview_url).unwrap_or(send_url);
    let id = present(input.id).unwrap_or(send_url);
    let title = input
        .title
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if content_type == "video/mp4" {
                "Saved clip".into()
            } else {
                "Saved GIF".into()
            }
        });
    let duration = input.duration.filter(|d| d.is_finite() && *d > 0.0);
    let asset = |url: &str| json!({ "url": url, "width": width, "height": height, "sizeBytes": size_bytes, "contentType": content_type });
    let mut item = json!({
        "id": id,
        "title": title,
        "provider": "external",
        "preview": asset(preview_url),
        "send": asset(send_url),
        "sourceUrl": input.source_url,
        "aspectRatio": width / height,
    });
    if let Some(alt) = input.alt_text.map(str::trim).filter(|a| !a.is_empty()) {
        item["altText"] = json!(alt);
    }
    if let Some(duration) = duration {
        item["duration"] = json!(duration);
    }
    // Inferred before the title and id defaults apply, as the web does.
    let probe = json!({
        "id": id,
        "title": input.title.unwrap_or_default(),
        "sourceUrl": input.source_url,
        "duration": duration,
        "preview": { "url": preview_url, "contentType": content_type },
        "send": { "url": send_url, "contentType": content_type },
    });
    item["mediaType"] = json!(media_type(&probe));
    item
}

/// `createAttachmentGifFavorite`.
pub fn attachment_gif(input: &FavoriteInput<'_>) -> Value {
    let mut item = external(input);
    let identity = present(input.file_key_or_url)
        .or(present(input.send_url))
        .unwrap_or(input.source_url);
    if let Some(x) = x_gif_source_url(Some(identity)) {
        item["id"] = json!(format!("x-media-0-{x}"));
        item["provider"] = json!("external");
    } else if let Some(provider @ ("klipy" | "tenor")) = attachment_provider(Some(identity)) {
        let id = filename_stem(input.filename)
            .or_else(|| present(input.id).map(str::to_owned))
            .unwrap_or_else(|| item["id"].as_str().unwrap_or_default().to_owned());
        item["id"] = json!(id);
        item["provider"] = json!(provider);
    }
    item["mediaType"] = json!(media_type(&item));
    item
}

/// `createAttachmentClipFavorite`.
pub fn attachment_clip(input: &FavoriteInput<'_>) -> Value {
    let identity = present(input.file_key_or_url)
        .or(present(input.send_url))
        .unwrap_or(input.source_url);
    let stem = filename_stem(input.filename);
    let title = input
        .title
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .or_else(|| stem.clone());
    let mut item = external(&FavoriteInput {
        title: title.as_deref(),
        content_type: Some("video/mp4"),
        ..input.clone()
    });
    let provider = attachment_provider(Some(identity));
    let id = match provider {
        Some("klipy" | "tenor") => stem
            .or_else(|| present(input.id).map(str::to_owned))
            .unwrap_or_else(|| item["id"].as_str().unwrap_or_default().to_owned()),
        _ => present(Some(input.source_url))
            .or(present(input.send_url))
            .or(present(input.id))
            .map(str::to_owned)
            .unwrap_or_else(|| item["id"].as_str().unwrap_or_default().to_owned()),
    };
    item["id"] = json!(id);
    item["provider"] = json!(provider.unwrap_or("external"));
    item["mediaType"] = json!("clips");
    item
}

/// `getGifItemIdentityKey`: `provider:id`.
pub fn identity(item: &Value) -> Option<String> {
    let provider = item["provider"].as_str().filter(|p| !p.is_empty())?;
    let id = item["id"].as_str().filter(|i| !i.is_empty())?;
    Some(format!("{provider}:{id}"))
}

/// `getFavoriteActionLabel`.
pub fn action_label(item: &Value, saved: bool) -> String {
    let clip = item["mediaType"].as_str() == Some("clips")
        || item["send"]["contentType"].as_str() == Some("video/mp4");
    let noun = if clip { "clip" } else { "GIF" };
    if saved {
        format!("Remove {noun} from favorites")
    } else {
        format!("Add {noun} to favorites")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(source: &str) -> FavoriteInput<'_> {
        FavoriteInput {
            source_url: source,
            ..FavoriteInput::default()
        }
    }

    #[test]
    fn a_klipy_mp4_should_save_as_a_klipy_gif_by_its_file_stem() {
        let item = attachment_gif(&FavoriteInput {
            filename: Some("happy-cat.mp4"),
            file_key_or_url: Some("attachments/gifs/klipy/happy-cat.mp4"),
            content_type: Some("video/mp4"),
            ..input("https://yap.115jon.com/api/attachments/gifs/klipy/happy-cat.mp4")
        });
        assert_eq!(identity(&item).as_deref(), Some("klipy:happy-cat"));
    }

    #[test]
    fn an_x_gif_should_save_by_its_twimg_mp4() {
        let item = attachment_gif(&FavoriteInput {
            file_key_or_url: Some("https://video.twimg.com/tweet_video/AbC.mp4"),
            ..input("https://video.twimg.com/tweet_video/AbC.mp4")
        });
        assert_eq!(
            item["id"],
            json!("x-media-0-https://video.twimg.com/tweet_video/AbC.mp4")
        );
    }

    #[test]
    fn a_plain_mp4_attachment_should_save_as_an_external_clip() {
        let item = attachment_clip(&FavoriteInput {
            filename: Some("clip.mp4"),
            ..input("https://yap.115jon.com/api/attachments/a/clip.mp4")
        });
        assert_eq!(
            (
                identity(&item),
                item["mediaType"].clone(),
                item["title"].clone()
            ),
            (
                Some("external:https://yap.115jon.com/api/attachments/a/clip.mp4".into()),
                json!("clips"),
                json!("clip")
            )
        );
    }

    #[test]
    fn an_external_gif_should_default_its_size_and_title() {
        let item = external(&input("https://gif.fxtwitter.com/tweet_video/x.webp"));
        assert_eq!(
            (
                item["send"]["width"].clone(),
                item["title"].clone(),
                item["mediaType"].clone()
            ),
            (json!(320.0), json!("Saved GIF"), json!("gifs"))
        );
    }

    #[test]
    fn the_webp_of_an_x_gif_should_come_from_fxtwitter() {
        assert_eq!(
            fx_twitter_webp_url("https://video.twimg.com/tweet_video/AbC.mp4").as_deref(),
            Some("https://gif.fxtwitter.com/tweet_video/AbC.webp")
        );
    }

    #[test]
    fn a_proxied_url_should_unwrap_to_its_source() {
        assert_eq!(
            unwrap_proxy_media_url(
                "https://yap.115jon.com/api/proxy-media?url=https%3A%2F%2Fvideo.twimg.com%2Fa.mp4"
            ),
            "https://video.twimg.com/a.mp4"
        );
    }

    #[test]
    fn the_label_should_name_a_clip() {
        let item = attachment_clip(&input("https://x/a.mp4"));
        assert_eq!(action_label(&item, true), "Remove clip from favorites");
    }
}
