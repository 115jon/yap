//! HTTP details of the stream route and the web's recovery plan
//! (`planListenTogetherPlaybackRecovery` in
//! `src/lib/voice/listen-together-player.ts`), ported as is.

/// `bytes=start-end` (inclusive).
pub(crate) fn range_header(start: u64, end_inclusive: u64) -> String {
    format!("bytes={start}-{end_inclusive}")
}

/// `Content-Range: bytes a-b/total` (or `/*`) -> (a, b, total).
pub(crate) fn parse_content_range(value: &str) -> Option<(u64, u64, Option<u64>)> {
    let rest = value.trim().strip_prefix("bytes")?.trim_start();
    let (range, total) = rest.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let start = start.trim().parse().ok()?;
    let end = end.trim().parse().ok()?;
    let total = match total.trim() {
        "*" => None,
        total => Some(total.parse().ok()?),
    };
    (end >= start).then_some((start, end, total))
}

/// `Retry-After` in seconds, as the web reads it (`Number(value)`, finite, > 0).
pub(crate) fn parse_retry_after(value: &str) -> Option<f64> {
    let seconds: f64 = value.trim().parse().ok()?;
    (seconds.is_finite() && seconds > 0.0).then_some(seconds)
}

/// The same URL with the other container (`format=mp4` <-> `format=webm`;
/// no `format` means the route's default, mp4).
pub(crate) fn alternate_format_url(url: &str) -> Option<String> {
    let mut parsed = reqwest::Url::parse(url).ok()?;
    let current = parsed
        .query_pairs()
        .find(|(name, _)| name == "format")
        .map(|(_, value)| value.into_owned());
    let next = if current.as_deref() == Some("webm") {
        "mp4"
    } else {
        "webm"
    };
    let pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .filter(|(name, _)| name != "format")
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect();
    {
        let mut query = parsed.query_pairs_mut();
        query.clear();
        for (name, value) in &pairs {
            query.append_pair(name, value);
        }
        query.append_pair("format", next);
    }
    Some(parsed.into())
}

/// The `format` query value, for hints and logs.
pub(crate) fn format_of(url: &str) -> Option<String> {
    reqwest::Url::parse(url).ok().and_then(|parsed| {
        parsed
            .query_pairs()
            .find(|(name, _)| name == "format")
            .map(|(_, value)| value.into_owned())
    })
}

pub(crate) const MAX_TRANSIENT_RETRIES: u32 = 3;
const DEFAULT_BLOCKED_RETRY_SECONDS: f64 = 20.0;
const MAX_RETRY_DELAY_MS: u64 = 60_000;
const NETWORK_RETRY_DELAY_MS: u64 = 3_000;

/// The web's `MediaError` codes that matter here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MediaError {
    Network,
    Decode,
    SrcNotSupported,
}

/// What the stream route answered (web: `ListenTogetherStreamProbe`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Probe {
    pub status: u16,
    pub retry_after_secs: Option<f64>,
}

/// Attempts spent on the current track; reset when it plays or changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RecoveryState {
    pub transient_retries: u32,
    pub format_switched: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    /// Same source after `delay_ms`.
    Retry,
    /// The other container now.
    SwitchFormat,
    Fail,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Recovery {
    pub action: Action,
    pub delay_ms: u64,
    pub code: &'static str,
    pub message: String,
}

fn retry(delay_ms: u64, message: impl Into<String>) -> Recovery {
    Recovery {
        action: Action::Retry,
        delay_ms,
        code: "PLAYBACK_RETRYING",
        message: message.into(),
    }
}

fn switch_format(message: &str) -> Recovery {
    Recovery {
        action: Action::SwitchFormat,
        delay_ms: 0,
        code: "PLAYBACK_RETRYING",
        message: message.into(),
    }
}

fn fail(code: &'static str, message: &str) -> Recovery {
    Recovery {
        action: Action::Fail,
        delay_ms: 0,
        code,
        message: message.into(),
    }
}

/// Port of `planListenTogetherPlaybackRecovery`. `probe` is None when the
/// route was unreachable.
pub(crate) fn plan_recovery(
    media_error: Option<MediaError>,
    probe: Option<Probe>,
    state: RecoveryState,
    can_try_alternate_format: bool,
) -> Recovery {
    let retries_left = state.transient_retries < MAX_TRANSIENT_RETRIES;
    let backoff = 2u64.saturating_pow(state.transient_retries);
    let can_switch = can_try_alternate_format && !state.format_switched;

    let Some(probe) = probe else {
        return if retries_left {
            retry(
                NETWORK_RETRY_DELAY_MS.saturating_mul(backoff),
                "Lost the audio stream. Reconnecting.",
            )
        } else {
            fail(
                "PLAYBACK_FAILED",
                "Couldn't reach the audio stream. Check your connection and try again.",
            )
        };
    };

    if probe.status == 401 || probe.status == 403 {
        return fail(
            "PLAYBACK_SESSION",
            "Listening needs an active voice connection. Rejoin the channel to keep listening.",
        );
    }

    if probe.status == 429 || probe.status == 503 {
        if !retries_left {
            return fail(
                "PLAYBACK_UNAVAILABLE",
                "YouTube is refusing our server right now. Try this track again in a few minutes.",
            );
        }
        let seconds = probe
            .retry_after_secs
            .unwrap_or(DEFAULT_BLOCKED_RETRY_SECONDS);
        let delay_ms = ((seconds * 1000.0 * backoff as f64).round() as u64).min(MAX_RETRY_DELAY_MS);
        let shown = (delay_ms as f64 / 1000.0).round() as u64;
        return retry(
            delay_ms,
            format!("YouTube is refusing our server for a moment. Retrying in {shown}s."),
        );
    }

    if probe.status >= 400 {
        return if can_switch {
            switch_format("That audio stream failed. Trying another format.")
        } else {
            fail(
                "PLAYBACK_UNAVAILABLE",
                "YouTube didn't give us a playable stream for this track. Skip it or queue another version.",
            )
        };
    }

    if matches!(
        media_error,
        Some(MediaError::Decode | MediaError::SrcNotSupported)
    ) {
        return if can_switch {
            switch_format("That audio format didn't play here. Trying another one.")
        } else {
            fail(
                "PLAYBACK_SRC_NOT_SUPPORTED",
                "This device can't play the audio formats YouTube offers for this track.",
            )
        };
    }

    if media_error == Some(MediaError::Network) && retries_left {
        return retry(
            NETWORK_RETRY_DELAY_MS.saturating_mul(backoff),
            "Lost the audio stream. Reconnecting.",
        );
    }

    fail(
        "PLAYBACK_FAILED",
        "Playback stopped and couldn't recover. Skip the track or try again.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(status: u16) -> Option<Probe> {
        Some(Probe {
            status,
            retry_after_secs: None,
        })
    }

    fn state(transient_retries: u32, format_switched: bool) -> RecoveryState {
        RecoveryState {
            transient_retries,
            format_switched,
        }
    }

    #[test]
    fn range_header_is_inclusive() {
        assert_eq!(range_header(0, 524_287), "bytes=0-524287");
    }

    #[test]
    fn content_range_parses_known_and_unknown_totals() {
        assert_eq!(
            parse_content_range("bytes 0-99/1234"),
            Some((0, 99, Some(1234)))
        );
        assert_eq!(parse_content_range("bytes 5-9/*"), Some((5, 9, None)));
        assert_eq!(parse_content_range("bytes */1234"), None);
        assert_eq!(parse_content_range("bytes 9-5/10"), None);
    }

    #[test]
    fn retry_after_must_be_positive() {
        assert_eq!(parse_retry_after("20"), Some(20.0));
        assert_eq!(parse_retry_after("0"), None);
        assert_eq!(parse_retry_after("Wed, 21 Oct 2026 07:28:00 GMT"), None);
    }

    #[test]
    fn alternate_format_swaps_the_container() {
        let mp4 =
            "https://yap.115jon.com/api/listen-together/stream?videoId=a&format=mp4&roomSlug=r";
        let webm = alternate_format_url(mp4).unwrap_or_default();
        assert!(webm.ends_with("videoId=a&roomSlug=r&format=webm"), "{webm}");
        assert_eq!(format_of(&webm).as_deref(), Some("webm"));
        let back = alternate_format_url(&webm).unwrap_or_default();
        assert_eq!(format_of(&back).as_deref(), Some("mp4"));
        let bare = alternate_format_url("https://h/x?videoId=a").unwrap_or_default();
        assert_eq!(format_of(&bare).as_deref(), Some("webm"));
    }

    #[test]
    fn unreachable_route_retries_three_times_then_fails() {
        let delays: Vec<u64> = (0..3)
            .map(|n| plan_recovery(None, None, state(n, false), true).delay_ms)
            .collect();
        assert_eq!(delays, [3000, 6000, 12_000]);
        let last = plan_recovery(None, None, state(3, false), true);
        assert_eq!((last.action, last.code), (Action::Fail, "PLAYBACK_FAILED"));
    }

    #[test]
    fn unauthorized_is_a_session_failure() {
        for status in [401, 403] {
            let plan = plan_recovery(None, probe(status), state(0, false), true);
            assert_eq!((plan.action, plan.code), (Action::Fail, "PLAYBACK_SESSION"));
            assert!(plan
                .message
                .starts_with("Listening needs an active voice connection"));
        }
    }

    #[test]
    fn blocked_retries_with_retry_after_backoff_capped_at_a_minute() {
        let blocked = |retry_after_secs, n| {
            plan_recovery(
                None,
                Some(Probe {
                    status: 503,
                    retry_after_secs,
                }),
                state(n, false),
                true,
            )
        };
        assert_eq!(blocked(Some(5.0), 0).delay_ms, 5000);
        assert_eq!(blocked(Some(5.0), 2).delay_ms, 20_000);
        assert_eq!(blocked(None, 0).delay_ms, 20_000);
        assert_eq!(blocked(None, 1).delay_ms, 40_000);
        assert_eq!(blocked(None, 2).delay_ms, 60_000);
        assert_eq!(
            blocked(None, 1).message,
            "YouTube is refusing our server for a moment. Retrying in 40s."
        );
        let exhausted = plan_recovery(None, probe(429), state(3, false), true);
        assert_eq!(
            (exhausted.action, exhausted.code),
            (Action::Fail, "PLAYBACK_UNAVAILABLE")
        );
    }

    #[test]
    fn other_errors_switch_format_once_then_fail() {
        let first = plan_recovery(None, probe(502), state(0, false), true);
        assert_eq!(
            (first.action, first.code),
            (Action::SwitchFormat, "PLAYBACK_RETRYING")
        );
        let second = plan_recovery(None, probe(502), state(0, true), true);
        assert_eq!(
            (second.action, second.code),
            (Action::Fail, "PLAYBACK_UNAVAILABLE")
        );
        let no_alternate = plan_recovery(None, probe(404), state(0, false), false);
        assert_eq!(no_alternate.action, Action::Fail);
    }

    #[test]
    fn undecodable_stream_switches_format_then_is_not_supported() {
        let first = plan_recovery(Some(MediaError::Decode), probe(200), state(0, false), true);
        assert_eq!(first.action, Action::SwitchFormat);
        assert_eq!(
            first.message,
            "That audio format didn't play here. Trying another one."
        );
        let second = plan_recovery(
            Some(MediaError::SrcNotSupported),
            probe(200),
            state(0, true),
            true,
        );
        assert_eq!(
            (second.action, second.code),
            (Action::Fail, "PLAYBACK_SRC_NOT_SUPPORTED")
        );
    }

    #[test]
    fn dropped_connection_on_a_healthy_route_retries() {
        let plan = plan_recovery(Some(MediaError::Network), probe(206), state(1, false), true);
        assert_eq!((plan.action, plan.delay_ms), (Action::Retry, 6000));
        let done = plan_recovery(Some(MediaError::Network), probe(206), state(3, false), true);
        assert_eq!((done.action, done.code), (Action::Fail, "PLAYBACK_FAILED"));
    }
}
