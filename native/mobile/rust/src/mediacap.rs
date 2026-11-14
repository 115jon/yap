//! Media capabilities (`?cap=m1....`, the server's `lib/media-capability.ts`)
//! for the players that cannot send the session as a header: Android's
//! video player and frame grabber (cpp/videoplayer.cpp). The server gives a
//! signed-in client a grant key for (user, expiry) at `POST
//! /api/media/grant`; each private URL is then signed with it, here, so a
//! URL names one object, one user and a few hours, and the session itself
//! never goes in a URL.
//!
//! Only the API's own private media is signed (`yap_core::api::
//! media_needs_auth`); anything else plays as it is. A grant belongs to the
//! session it was fetched under and is fetched again with less than two
//! hours left, counted on the server's clock (`issuedAt`).

use std::ffi::{c_char, c_void};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use reqwest::header::AUTHORIZATION;
use ring::hmac;

const CAP_VERSION: &str = "m1";
/// The web's `MEDIA_GRANT_REFRESH_MARGIN_MS`: a signed URL stays good for
/// at least this long (a video paused and resumed, a seek).
const REFRESH_MARGIN_MS: u64 = 2 * 60 * 60 * 1000;
/// After a failed grant request, how long the next one waits.
const RETRY_AFTER: Duration = Duration::from_secs(15);

struct Grant {
    user_id: String,
    /// Epoch milliseconds, on the server's clock.
    expires_at: u64,
    key: Vec<u8>,
    /// The server's clock when it was issued, and ours then.
    issued_at: u64,
    fetched: Instant,
    /// `yap_ui::media::token_generation` it was fetched under.
    generation: u64,
}

impl Grant {
    fn usable(&self, generation: u64) -> bool {
        let server_now = self.issued_at + self.fetched.elapsed().as_millis() as u64;
        self.generation == generation && server_now + REFRESH_MARGIN_MS < self.expires_at
    }
}

static GRANT: Mutex<Option<Grant>> = Mutex::new(None);
static LAST_FAILURE: Mutex<Option<Instant>> = Mutex::new(None);
// One grant request at a time: the rest wait for its answer.
static FETCH: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// What a cap signs: the path and the query without any `cap` or `token`
/// parameter, as the URL has them (`mediaCapSubject`). `None` for a URL
/// that does not parse.
fn subject_and_rest(url: &str) -> Option<(String, String)> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let kept: Vec<&str> = parsed
        .query()
        .unwrap_or_default()
        .split('&')
        .filter(|part| {
            let name = part.split('=').next().unwrap_or_default();
            !part.is_empty() && name != "cap" && name != "token"
        })
        .collect();
    let query = kept.join("&");
    let subject = if query.is_empty() {
        parsed.path().to_owned()
    } else {
        format!("{}?{query}", parsed.path())
    };
    let mut base = parsed.clone();
    base.set_query(None);
    base.set_fragment(None);
    let rest = if query.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{query}")
    };
    Some((subject, rest))
}

/// `url` with a cap for it under the grant (`withMediaCap`): any old
/// `cap`/`token` dropped, the cap last.
fn sign(user_id: &str, expires_at: u64, key: &[u8], url: &str) -> Option<String> {
    let (subject, rest) = subject_and_rest(url)?;
    let tag = hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, key), subject.as_bytes());
    let cap = format!(
        "{CAP_VERSION}.{}.{expires_at}.{}",
        URL_SAFE_NO_PAD.encode(user_id.as_bytes()),
        URL_SAFE_NO_PAD.encode(tag.as_ref())
    );
    let joiner = if rest.contains('?') { '&' } else { '?' };
    Some(format!("{rest}{joiner}cap={cap}"))
}

fn parse_grant(body: &serde_json::Value, generation: u64) -> Option<Grant> {
    let user_id = body.get("userId")?.as_str()?.to_owned();
    let expires_at = body.get("expiresAt")?.as_f64()? as u64;
    let key = URL_SAFE_NO_PAD.decode(body.get("key")?.as_str()?).ok()?;
    if user_id.is_empty() || key.is_empty() {
        return None;
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let issued_at = body
        .get("issuedAt")
        .and_then(serde_json::Value::as_f64)
        .map_or(now_ms, |v| v as u64);
    Some(Grant {
        user_id,
        expires_at,
        key,
        issued_at,
        fetched: Instant::now(),
        generation,
    })
}

/// Signs `url` with the held grant if it is still good.
fn sign_held(url: &str, generation: u64) -> Option<String> {
    let held = GRANT.lock().unwrap_or_else(PoisonError::into_inner);
    let grant = held.as_ref().filter(|g| g.usable(generation))?;
    sign(&grant.user_id, grant.expires_at, &grant.key, url)
}

/// `url` as a player can load it: itself when it needs no session, signed
/// when it does, `None` when it needs one and no grant can be had (signed
/// out, offline, the server refusing).
async fn playable(url: String) -> Option<String> {
    let api_base = yap_core::config::get().api_base.clone();
    if !yap_core::api::media_needs_auth(&api_base, &url) {
        return Some(url);
    }
    let generation = yap_ui::media::token_generation();
    if let Some(signed) = sign_held(&url, generation) {
        return Some(signed);
    }
    let _one = FETCH.lock().await;
    // Another caller may have fetched it while this one waited.
    if let Some(signed) = sign_held(&url, generation) {
        return Some(signed);
    }
    if LAST_FAILURE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .is_some_and(|at| at.elapsed() < RETRY_AFTER)
    {
        return None;
    }
    let token = yap_ui::media::token_for(&url)?;
    let endpoint = format!("{}/api/media/grant", api_base.trim_end_matches('/'));
    let answer = crate::http::client()
        .post(endpoint)
        .header(AUTHORIZATION, format!("Bearer {}", token.as_str()))
        .json(&serde_json::json!({}))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status);
    let grant = match answer {
        Ok(response) => response
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|body| parse_grant(&body, generation)),
        Err(_) => None,
    };
    // The session may have changed while the request was out.
    let Some(grant) = grant.filter(|_| yap_ui::media::token_generation() == generation) else {
        *LAST_FAILURE.lock().unwrap_or_else(PoisonError::into_inner) = Some(Instant::now());
        eprintln!("[media] no media grant");
        return None;
    };
    let signed = sign(&grant.user_id, grant.expires_at, &grant.key, &url);
    *GRANT.lock().unwrap_or_else(PoisonError::into_inner) = Some(grant);
    signed
}

/// Answers [`yap_media_playable`] once, from a runtime thread: the URL to
/// load (UTF-8, `len` bytes), or `len` 0 when it cannot be loaded.
pub type PlayableCallback = extern "C" fn(context: *mut c_void, url: *const c_char, len: usize);

struct Context(*mut c_void);
// SAFETY: the caller hands the context to whichever thread answers.
unsafe impl Send for Context {}

/// Resolves `url` (UTF-8, `len` bytes) for a player that sends no headers:
/// see [`playable`]. `done(context, ...)` is called exactly once, from
/// another thread.
///
/// # Safety
/// `url` must point to `len` readable bytes; `done` must be callable from
/// any thread with `context` until it has been called.
#[no_mangle]
pub unsafe extern "C" fn yap_media_playable(
    url: *const c_char,
    len: usize,
    done: PlayableCallback,
    context: *mut c_void,
) {
    let url = if url.is_null() || len == 0 {
        None
    } else {
        // SAFETY: the caller's guarantee.
        std::str::from_utf8(unsafe { std::slice::from_raw_parts(url.cast::<u8>(), len) })
            .ok()
            .map(str::to_owned)
    };
    let context = Context(context);
    crate::http::runtime().spawn(async move {
        let context = context;
        let answer = match url {
            Some(url) => playable(url).await,
            None => None,
        };
        let answer = answer.unwrap_or_default();
        done(context.0, answer.as_ptr().cast(), answer.len());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // From the server's own signMediaCap (src/lib/media-capability.ts) for
    // the same grant and URL: key "a2V5LWJ5dGVz" ("key-bytes"), user
    // "user-1", expiry 1767225600000.
    const KEY: &[u8] = b"key-bytes";

    #[test]
    fn signs_the_path_as_the_server_does() {
        let signed = sign(
            "user-1",
            1_767_225_600_000,
            KEY,
            "https://api.example/api/attachments/a/b.mp4",
        )
        .unwrap();
        assert_eq!(
            signed,
            "https://api.example/api/attachments/a/b.mp4?cap=m1.dXNlci0x.1767225600000.\
             CKwBhc6ikC6-xg9ax61xvIEfY0LQTLZOx59IL1AVBaw"
        );
    }

    #[test]
    fn drops_old_credentials_and_keeps_the_rest_of_the_query() {
        let signed = sign(
            "user-1",
            1_767_225_600_000,
            KEY,
            "https://api.example/api/attachments/x.webm?token=old&v=2&cap=stale#t=3",
        )
        .unwrap();
        let (subject, _) = subject_and_rest(&signed).unwrap();
        assert_eq!(subject, "/api/attachments/x.webm?v=2");
        assert!(signed.starts_with("https://api.example/api/attachments/x.webm?v=2&cap=m1."));
        assert!(!signed.contains("token="));
        assert_eq!(signed.matches("cap=").count(), 1);
    }

    #[test]
    fn a_grant_is_good_until_two_hours_before_it_ends() {
        let grant = |left_ms: u64| Grant {
            user_id: "u".into(),
            expires_at: 10_000_000_000 + left_ms,
            key: KEY.to_vec(),
            issued_at: 10_000_000_000,
            fetched: Instant::now(),
            generation: 3,
        };
        assert!(grant(3 * 60 * 60 * 1000).usable(3));
        assert!(!grant(3 * 60 * 60 * 1000).usable(4));
        assert!(!grant(REFRESH_MARGIN_MS - 1).usable(3));
    }

    #[test]
    fn a_grant_answer_parses() {
        let body = serde_json::json!({
            "userId": "user-1",
            "expiresAt": 1_767_225_600_000u64,
            "issuedAt": 1_767_211_200_000u64,
            "key": "a2V5LWJ5dGVz",
        });
        let grant = parse_grant(&body, 7).unwrap();
        assert_eq!(grant.key, KEY);
        assert_eq!(grant.generation, 7);
        assert!(parse_grant(&serde_json::json!({ "userId": "u" }), 7).is_none());
    }
}
