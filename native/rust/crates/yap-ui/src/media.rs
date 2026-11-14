//! Media plumbing the desktop and the phone share: the session the image
//! loaders send with the API's private media, and what the composer knows
//! about a local file before it uploads it.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{PoisonError, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use yap_core::{Token, UploadFile};

static MEDIA_TOKEN: RwLock<Option<Token>> = RwLock::new(None);
/// Bumped with every session change: private media an app keeps in memory
/// belongs to the session it was fetched under.
static TOKEN_GENERATION: AtomicU64 = AtomicU64::new(0);

/// The session for media requests (`ViewUpdate::MediaAuth`); `None` after
/// sign-out.
pub fn set_token(token: Option<Token>) {
    *MEDIA_TOKEN.write().unwrap_or_else(PoisonError::into_inner) = token;
    TOKEN_GENERATION.fetch_add(1, Ordering::AcqRel);
}

/// Which session private media is fetched under now (see [`set_token`]).
pub fn token_generation() -> u64 {
    TOKEN_GENERATION.load(Ordering::Acquire)
}

/// The session when `url` is the API's own private media (attachments,
/// custom emoji); `None` for anything else, which never gets it, and after
/// sign-out.
pub fn token_for(url: &str) -> Option<Token> {
    if !yap_core::api::media_needs_auth(&yap_core::config::get().api_base, url) {
        return None;
    }
    MEDIA_TOKEN
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// `Bearer <session>` for `url` when it is the API's own private media;
/// `None` for anything else, which never gets the session.
pub fn bearer_for(url: &str) -> Option<String> {
    bearer(
        &yap_core::config::get().api_base,
        url,
        token_for(url).as_ref(),
    )
}

/// `Bearer <session>` for `url` when it is the API at `api_base`'s private
/// media; `None` for anything else, or without a session.
pub fn bearer(api_base: &str, url: &str, token: Option<&Token>) -> Option<String> {
    if !yap_core::api::media_needs_auth(api_base, url) {
        return None;
    }
    token.map(|t| format!("Bearer {}", t.as_str()))
}

/// The MIME type the browser would send for a file name (the common ones,
/// photos and videos from a phone's picker included; the server stores
/// whatever it gets, `application/octet-stream` else).
pub fn content_type(name: &str) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" | "jfif" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        "heic" => "image/heic",
        "heif" => "image/heif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "3gp" => "video/3gpp",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "oga" => "audio/ogg",
        "opus" => "audio/opus",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "weba" => "audio/webm",
        "amr" => "audio/amr",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "7z" => "application/x-7z-compressed",
        "json" => "application/json",
        "txt" | "log" | "md" => "text/plain",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        _ => "application/octet-stream",
    }
}

/// Reads what an upload needs to know about each local file; folders and
/// files that cannot be read are skipped.
pub fn upload_files(paths: &[String]) -> Vec<UploadFile> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    paths
        .iter()
        .enumerate()
        .filter_map(|(i, path)| {
            let meta = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
            let name = std::path::Path::new(path)
                .file_name()?
                .to_string_lossy()
                .into_owned();
            Some(UploadFile {
                local_id: format!("upload-{stamp:x}-{i}"),
                path: path.clone(),
                content_type: content_type(&name).to_owned(),
                name,
                size: meta.len(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const API: &str = "https://api.example.com";

    #[test]
    fn bearer_only_for_the_apis_private_media() {
        let token = Token::new("t0k");
        let t = Some(&token);
        assert_eq!(
            bearer(API, "https://api.example.com/api/attachments/a/b.png", t).as_deref(),
            Some("Bearer t0k")
        );
        assert!(bearer(API, "https://api.example.com/api/emojis/assets/x.webp", t).is_some());
        // Public avatars, other hosts and look-alike hosts never get it.
        assert!(bearer(API, "https://api.example.com/api/avatars/u.png", t).is_none());
        assert!(bearer(API, "https://cdn.example.net/api/attachments/a.png", t).is_none());
        assert!(bearer(API, "https://api.example.com.evil.net/api/attachments/a", t).is_none());
        assert!(bearer(API, "https://api.example.com@evil.net/api/attachments/a", t).is_none());
        assert!(bearer(API, "http://api.example.com/api/attachments/a.png", t).is_none());
        // Signed out: nothing to send.
        assert!(bearer(API, "https://api.example.com/api/attachments/a.png", None).is_none());
    }

    #[test]
    fn a_session_change_bumps_the_generation() {
        let before = token_generation();
        set_token(None);
        assert!(token_generation() > before);
    }

    #[test]
    fn content_types_by_extension() {
        assert_eq!(content_type("a.PNG"), "image/png");
        assert_eq!(content_type("a.jfif"), "image/jpeg");
        assert_eq!(content_type("clip.heic"), "image/heic");
        assert_eq!(content_type("clip.3gp"), "video/3gpp");
        assert_eq!(content_type("song.m4a"), "audio/mp4");
        assert_eq!(content_type("memo.OPUS"), "audio/opus");
        assert_eq!(content_type("clip.aac"), "audio/aac");
        assert_eq!(content_type("notes.md"), "text/plain");
        assert_eq!(content_type("noext"), "application/octet-stream");
        assert_eq!(content_type("a.unknown"), "application/octet-stream");
    }

    #[test]
    fn upload_files_skip_folders_and_missing_paths() {
        let dir = std::env::temp_dir().join(format!("yap-ui-media-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("pic.jpeg");
        std::fs::write(&file, b"12345").unwrap();
        let paths = vec![
            file.to_string_lossy().into_owned(),
            dir.to_string_lossy().into_owned(),
            dir.join("missing.png").to_string_lossy().into_owned(),
        ];
        let files = upload_files(&paths);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "pic.jpeg");
        assert_eq!(files[0].content_type, "image/jpeg");
        assert_eq!(files[0].size, 5);
        assert!(files[0].local_id.starts_with("upload-") && files[0].local_id.ends_with("-0"));
    }
}
