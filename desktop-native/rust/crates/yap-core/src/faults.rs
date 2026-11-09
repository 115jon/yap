//! Test switches for the error states (UI parity pass 13): environment
//! variables read once by this process only. Each makes our own requests
//! fail the way a lost network, a down server or an error status would, so
//! the offline strip, "No chat link", a 403 channel, a failed upload, a
//! failed call join and a rate limit can be shown on demand. Nothing
//! system-wide is touched (no proxy, hosts file, firewall or adapter).
//!
//! Unset (the default) every check here answers "no fault" and nothing
//! changes. The API client ([`crate::api::Api`]) asks [`intercept`] before
//! it sends anything. The chat gateway and both call sockets get their
//! tickets through that client, so they fail to connect too, and the call
//! admission is a request like any other.
//!
//! - `YAP_TEST_OFFLINE=1`: every request fails at once as on a device with
//!   no network (a DNS failure, which [`ApiError::is_offline`] reads as
//!   offline): the "Offline" strip, "Waiting for network" at launch.
//! - `YAP_TEST_UNREACHABLE=1`: every request to the API host fails as a
//!   refused connection; the network is up, the server is not: the strip
//!   counts its tries, then says "No chat link".
//! - `YAP_TEST_HTTP_STATUS=<rule>[,<rule>...]`, each rule
//!   `[METHOD ]<substring>=<status>`: a request whose path (with its query)
//!   contains `substring` (ends with it, for a substring ending in `$`),
//!   and whose method is `METHOD` when one is given, is answered with
//!   `status` and the server's error message for it, without going out.
//!   The first matching rule wins. For example `POST /messages$=429`
//!   (sending a message, not an upload), `/messages/upload=413`,
//!   `GET /api/channels/<id>/messages=403`.
//! - `YAP_TEST_VOICE_FAIL=admission|ice`: a call join fails at that stage.
//!   `admission`: `POST /api/voice/admission` answers 500 (the app's
//!   admission client). `ice`: the media connection never comes up (it
//!   reports failed, so it is rebuilt and finally given up; yap-voice reads
//!   the same variable in its driver).

use std::sync::OnceLock;

use crate::api::ApiError;

/// What [`intercept`] answers with under `YAP_TEST_OFFLINE`: the shape of a
/// real DNS failure, so everything downstream treats it as one.
pub const OFFLINE_ERROR: &str =
    "error sending request: client error (Connect): dns error: no network (YAP_TEST_OFFLINE)";

/// Under `YAP_TEST_UNREACHABLE`: a refused connection to the API host.
pub const UNREACHABLE_ERROR: &str =
    "error sending request: client error (Connect): tcp connect error: connection refused (YAP_TEST_UNREACHABLE)";

/// One `YAP_TEST_HTTP_STATUS` rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusRule {
    /// Upper case; `None` matches any method.
    pub method: Option<String>,
    pub needle: String,
    pub status: u16,
}

impl StatusRule {
    fn matches(&self, method: &str, path: &str) -> bool {
        self.method
            .as_deref()
            .is_none_or(|wanted| wanted.eq_ignore_ascii_case(method))
            && match self.needle.strip_suffix('$') {
                Some(end) => path.ends_with(end),
                None => path.contains(&self.needle),
            }
    }
}

/// The stage `YAP_TEST_VOICE_FAIL` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceFail {
    Admission,
    Ice,
}

/// Every switch, as read from the environment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Switches {
    pub offline: bool,
    pub unreachable: bool,
    pub statuses: Vec<StatusRule>,
    pub voice: Option<VoiceFail>,
}

/// `1`, `true`, `yes` or `on` (any case) turn a flag on.
pub fn parse_flag(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// `YAP_TEST_HTTP_STATUS`'s rules; a malformed one (no `=`, a status that
/// is not 100-599, an empty substring) is skipped.
pub fn parse_status_rules(value: &str) -> Vec<StatusRule> {
    value
        .split(',')
        .filter_map(|rule| {
            let (target, status) = rule.trim().rsplit_once('=')?;
            let status: u16 = status.trim().parse().ok()?;
            if !(100..=599).contains(&status) {
                return None;
            }
            let target = target.trim();
            let (method, needle) = match target.split_once(' ') {
                Some((method, needle))
                    if !method.is_empty() && method.chars().all(|c| c.is_ascii_alphabetic()) =>
                {
                    (Some(method.to_ascii_uppercase()), needle.trim())
                }
                _ => (None, target),
            };
            (!needle.is_empty()).then(|| StatusRule {
                method,
                needle: needle.to_owned(),
                status,
            })
        })
        .collect()
}

pub fn parse_voice_fail(value: &str) -> Option<VoiceFail> {
    match value.trim().to_ascii_lowercase().as_str() {
        "admission" => Some(VoiceFail::Admission),
        "ice" | "media" => Some(VoiceFail::Ice),
        _ => None,
    }
}

impl Switches {
    fn from_env() -> Self {
        let var = |name: &str| std::env::var(name).unwrap_or_default();
        let switches = Self {
            offline: parse_flag(&var("YAP_TEST_OFFLINE")),
            unreachable: parse_flag(&var("YAP_TEST_UNREACHABLE")),
            statuses: parse_status_rules(&var("YAP_TEST_HTTP_STATUS")),
            voice: parse_voice_fail(&var("YAP_TEST_VOICE_FAIL")),
        };
        if switches != Self::default() {
            eprintln!("[test] fault switches on: {switches:?}");
        }
        switches
    }

    /// The fault for one request, if any: `url` is the whole URL.
    pub fn fault(&self, api_base: &str, method: &str, url: &str) -> Option<ApiError> {
        if self.offline {
            return Some(ApiError::Network(OFFLINE_ERROR.into()));
        }
        let base = api_base.trim_end_matches('/');
        let on_api = !base.is_empty() && url.starts_with(base);
        if self.unreachable && on_api {
            return Some(ApiError::Network(UNREACHABLE_ERROR.into()));
        }
        let path = path_of(url);
        let status = self
            .statuses
            .iter()
            .find(|rule| rule.matches(method, path))?
            .status;
        Some(status_error(status))
    }
}

/// The part of `url` after its host: the path and the query.
fn path_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.find('/').map_or("/", |at| &rest[at..])
}

/// What the server answers for `status` (its `{ "error": ... }` text), as
/// the client decodes it.
pub fn status_error(status: u16) -> ApiError {
    let message = match status {
        401 => return ApiError::Unauthorized,
        // require-channel-access.ts
        403 => "Channel not found or access denied",
        // voice-move.ts and the channel routes
        404 => "Channel not found",
        // channels/$id/messages/upload.ts
        413 => "File too large (max 25MB)",
        // rate-limit.ts (with Retry-After)
        429 => "Rate limit exceeded. Please slow down.",
        // edge.ts
        500 => "Internal server error",
        _ => "request failed",
    };
    ApiError::Http {
        status,
        message: message.into(),
    }
}

/// The switches of this process, read on first use.
pub fn switches() -> &'static Switches {
    static SWITCHES: OnceLock<Switches> = OnceLock::new();
    SWITCHES.get_or_init(Switches::from_env)
}

/// The fault for a request about to go out, if a switch says so.
pub fn intercept(method: &str, url: &str) -> Option<ApiError> {
    switches().fault(&crate::config::get().api_base, method, url)
}

/// `YAP_TEST_VOICE_FAIL`'s stage, if set.
pub fn voice_fail() -> Option<VoiceFail> {
    switches().voice
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://yap.test";

    #[test]
    fn flags_should_read_the_usual_spellings() {
        for on in ["1", "true", "YES", " on "] {
            assert!(parse_flag(on), "{on}");
        }
        for off in ["", "0", "false", "off", "2"] {
            assert!(!parse_flag(off), "{off}");
        }
    }

    #[test]
    fn status_rules_should_parse_method_substring_and_status() {
        assert_eq!(
            parse_status_rules(
                "POST /messages=429, /messages/upload=413,get /api/gifs?mode=search=500"
            ),
            vec![
                StatusRule {
                    method: Some("POST".into()),
                    needle: "/messages".into(),
                    status: 429
                },
                StatusRule {
                    method: None,
                    needle: "/messages/upload".into(),
                    status: 413
                },
                StatusRule {
                    method: Some("GET".into()),
                    needle: "/api/gifs?mode=search".into(),
                    status: 500
                },
            ]
        );
    }

    #[test]
    fn malformed_status_rules_should_be_skipped() {
        assert!(parse_status_rules("").is_empty());
        assert!(parse_status_rules("/messages").is_empty());
        assert!(parse_status_rules("/messages=abc,/x=99,/y=600,=403, =500").is_empty());
        assert_eq!(parse_status_rules("bad,/ok=404").len(), 1);
    }

    #[test]
    fn voice_stages_should_parse() {
        assert_eq!(parse_voice_fail("admission"), Some(VoiceFail::Admission));
        assert_eq!(parse_voice_fail(" ICE "), Some(VoiceFail::Ice));
        assert_eq!(parse_voice_fail("media"), Some(VoiceFail::Ice));
        assert_eq!(parse_voice_fail(""), None);
        assert_eq!(parse_voice_fail("sfu"), None);
    }

    #[test]
    fn no_switch_should_fault_nothing() {
        let off = Switches::default();
        assert_eq!(
            off.fault(BASE, "GET", "https://yap.test/api/users/me"),
            None
        );
    }

    #[test]
    fn offline_should_fail_every_host_as_no_network() {
        let switches = Switches {
            offline: true,
            ..Switches::default()
        };
        for url in ["https://yap.test/api/users/me", "https://auth.test/x"] {
            let error = switches.fault(BASE, "GET", url).expect("fault");
            assert!(error.is_offline(), "{error}");
        }
    }

    #[test]
    fn unreachable_should_fail_the_api_host_only() {
        let switches = Switches {
            unreachable: true,
            ..Switches::default()
        };
        let error = switches
            .fault(BASE, "POST", "https://yap.test/api/voice/socket-ticket")
            .expect("fault");
        assert!(matches!(error, ApiError::Network(_)));
        assert!(!error.is_offline());
        assert_eq!(switches.fault(BASE, "GET", "https://auth.test/api/x"), None);
    }

    #[test]
    fn a_status_rule_should_answer_with_the_server_error() {
        let switches = Switches {
            statuses: parse_status_rules("POST /messages$=429,/c1/messages=403"),
            ..Switches::default()
        };
        assert_eq!(
            switches.fault(BASE, "POST", "https://yap.test/api/channels/c2/messages"),
            Some(ApiError::Http {
                status: 429,
                message: "Rate limit exceeded. Please slow down.".into()
            })
        );
        // `$` keeps the upload out of the send's rule.
        assert_eq!(
            switches.fault(
                BASE,
                "POST",
                "https://yap.test/api/channels/c2/messages/upload"
            ),
            None
        );
        // The method narrows the first rule; the second matches any.
        assert_eq!(
            switches.fault(
                BASE,
                "GET",
                "https://yap.test/api/channels/c2/messages?limit=50"
            ),
            None
        );
        assert!(matches!(
            switches.fault(
                BASE,
                "GET",
                "https://yap.test/api/channels/c1/messages?limit=50"
            ),
            Some(ApiError::Http { status: 403, .. })
        ));
    }

    #[test]
    fn a_401_rule_should_read_as_an_ended_session() {
        assert_eq!(status_error(401), ApiError::Unauthorized);
    }

    #[test]
    fn the_path_should_include_the_query() {
        assert_eq!(
            path_of("https://h.test/api/gifs?mode=search"),
            "/api/gifs?mode=search"
        );
        assert_eq!(path_of("https://h.test"), "/");
    }
}
