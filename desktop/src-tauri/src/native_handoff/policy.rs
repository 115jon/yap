//! The native handoff's rules, pure: no I/O, no Tauri, no Win32, so every
//! decision is unit-tested on any host (`native_handoff` does the I/O).
//!
//! - which GitHub release is the native app ([`pick_native_release`]);
//! - the remote switch and the per-install rollout bucket ([`switch_admits`]);
//! - the Setup digest ([`digest_matches`]) and its signer ([`check_signer`]);
//! - the marker file's state machine ([`Marker`]);
//! - when the started native app counts as alive ([`judge_liveness`]);
//! - the minimum Windows build ([`windows_build_supported`]).

use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

/// The native installer's asset name on the release (Velopack, packId
/// `Yap.App`, channel `win`).
pub const ASSET_NAME: &str = "Yap.App-win-Setup.exe";

/// The asset sizes accepted, in bytes (10 MB to 500 MB).
pub const MIN_SETUP_BYTES: u64 = 10_000_000;
pub const MAX_SETUP_BYTES: u64 = 500_000_000;

/// The first major version that is the native app (Tauri ends at 3.x).
pub const MIN_NATIVE_MAJOR: u64 = 4;

/// Where a discovered (not overridden) release's asset must download from.
pub const GITHUB_DOWNLOAD_PREFIX: &str = "https://github.com/115jon/yap/releases/download/";

/// The signer's CN on every Yap binary (Azure Artifact Signing, individual
/// identity). Not a thumbprint: those certificates last 3 days.
pub const SIGNER_NAME: &str = "Jonathan Martinez";

/// The start of the issuing CA's CN for Artifact Signing's public-trust
/// certificates ("Microsoft ID Verified CS AOC CA 03", "... CA 04", ...).
pub const ISSUER_PREFIX: &str = "Microsoft ID Verified CS";

/// The code-signing EKU.
pub const CODE_SIGNING_EKU: &str = "1.3.6.1.5.5.7.3.3";

/// The EKU Artifact Signing writes for this account's identity validation
/// (`1.3.6.1.4.1.311.97.` + the validation's id). Every certificate issued
/// under that validation carries it: the 3.13.0 Setup (certificate of
/// 2026-11-03, CA 03, thumbprint 1A2BE3F0...) and the native 0.2.25 Setup
/// (2026-11-08, CA 04, 6EC73707...) both do. A new identity validation
/// would change it, and this bridge with it. The generic
/// `1.3.6.1.4.1.311.97.1.0` (any public-trust customer) is not enough.
pub const IDENTITY_EKU: &str = "1.3.6.1.4.1.311.97.726408799.228623495.642435689.37395988";

/// Failed attempts after which the launch stops installing by itself.
pub const MAX_AUTO_ATTEMPTS: u32 = 3;

/// Failed redirects to an installed native app before a launch starts this
/// app instead.
pub const MAX_REDIRECT_FAILURES: u32 = 2;

/// How long a "not yet" answer from GitHub stands before asking again.
pub const NOT_YET_BACKOFF: Duration = Duration::from_secs(6 * 60 * 60);

/// How long an unreachable GitHub stands before asking again.
pub const ERROR_BACKOFF: Duration = Duration::from_secs(60 * 60);

/// How long a discovered release is used before GitHub is asked again (a
/// newer 4.x may be out by then).
pub const RELEASE_CACHE_TTL: Duration = Duration::from_secs(6 * 60 * 60);

/// How long the native app has to stay up after it is started.
pub const LIVENESS_WINDOW: Duration = Duration::from_secs(8);

/// The first Windows build the native app supports: Windows 10 1809, the
/// floor of Qt 6.11 (the static Qt it links). Its manifest declares Windows
/// 10 and 11 only.
pub const MIN_WINDOWS_BUILD: u32 = 17763;

// ── Release discovery ────────────────────────────────────────────────────

/// The parts of a GitHub release object this reads.
#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    size: u64,
    browser_download_url: String,
    /// `"sha256:<hex>"`, which GitHub computes for every uploaded asset.
    #[serde(default)]
    digest: Option<String>,
}

/// A native release the bridge may install.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeRelease {
    /// `4.0.0`, from the tag `v4.0.0`.
    pub version: String,
    pub url: String,
    /// Lower-case hex.
    pub sha256: String,
    pub size: u64,
}

/// Why a release is not (yet) one to install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotYet {
    Unreadable(String),
    Tag(String),
    OldMajor(String),
    Draft,
    Prerelease,
    NoAsset,
    NoDigest,
    Size(u64),
    Url(String),
    /// No release in the list passes (`seen` releases were looked at).
    NoNativeRelease {
        seen: usize,
    },
}

impl fmt::Display for NotYet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable(e) => write!(f, "releases not readable: {e}"),
            Self::Tag(t) => write!(f, "tag {t:?} is not vX.Y.Z"),
            Self::OldMajor(t) => write!(f, "{t} is not the native app"),
            Self::Draft => f.write_str("the release is a draft"),
            Self::Prerelease => f.write_str("the release is a prerelease"),
            Self::NoAsset => write!(f, "no {ASSET_NAME} asset"),
            Self::NoDigest => write!(f, "{ASSET_NAME} has no sha256 digest"),
            Self::Size(n) => write!(f, "{ASSET_NAME} is {n} bytes"),
            Self::Url(u) => write!(f, "{ASSET_NAME} downloads from {u}"),
            Self::NoNativeRelease { seen } => {
                write!(f, "no native release yet among the {seen} latest releases")
            }
        }
    }
}

/// `(major, minor, patch)` of a tag that is exactly `v<digits>.<digits>.<digits>`.
pub fn parse_tag(tag: &str) -> Option<(u64, u64, u64)> {
    let rest = tag.strip_prefix('v')?;
    let mut parts = rest.split('.');
    let mut next = || -> Option<u64> {
        let part = parts.next()?;
        if part.is_empty() || part.len() > 9 || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        part.parse().ok()
    };
    let version = (next()?, next()?, next()?);
    parts.next().is_none().then_some(version)
}

/// The lower-case hex of a `sha256:<64 hex>` digest.
pub fn parse_digest(digest: &str) -> Option<String> {
    let hex = digest.strip_prefix("sha256:")?;
    (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| hex.to_ascii_lowercase())
}

/// The one asset named exactly [`ASSET_NAME`], with a digest and a sane
/// size. `github_only`: its URL must be this repository's release download.
fn select_asset(assets: &[GhAsset], github_only: bool) -> Result<(String, u64, String), NotYet> {
    let asset = assets
        .iter()
        .find(|a| a.name == ASSET_NAME)
        .ok_or(NotYet::NoAsset)?;
    let sha256 = asset
        .digest
        .as_deref()
        .and_then(parse_digest)
        .ok_or(NotYet::NoDigest)?;
    if !(MIN_SETUP_BYTES..=MAX_SETUP_BYTES).contains(&asset.size) {
        return Err(NotYet::Size(asset.size));
    }
    let url = &asset.browser_download_url;
    let allowed = if github_only {
        url.starts_with(GITHUB_DOWNLOAD_PREFIX)
    } else {
        url.starts_with("https://") || url.starts_with("http://")
    };
    if !allowed {
        return Err(NotYet::Url(url.clone()));
    }
    Ok((url.clone(), asset.size, sha256))
}

/// One release object, accepted only if every rule holds: tag `vX.Y.Z` with
/// X >= 4, neither draft nor prerelease, and an asset [`ASSET_NAME`] with a
/// `sha256:` digest and a size of 10–500 MB. `github_only` is off only for
/// the test override (`YAP_HANDOFF_RELEASE`).
#[cfg(test)]
pub fn accept_release(json: &[u8], github_only: bool) -> Result<NativeRelease, NotYet> {
    let release: GhRelease =
        serde_json::from_slice(json).map_err(|e| NotYet::Unreadable(e.to_string()))?;
    accept(&release, github_only).map(|(_, r)| r)
}

fn accept(
    release: &GhRelease,
    github_only: bool,
) -> Result<((u64, u64, u64), NativeRelease), NotYet> {
    let version =
        parse_tag(&release.tag_name).ok_or_else(|| NotYet::Tag(release.tag_name.clone()))?;
    let (major, minor, patch) = version;
    if major < MIN_NATIVE_MAJOR {
        return Err(NotYet::OldMajor(release.tag_name.clone()));
    }
    if release.draft {
        return Err(NotYet::Draft);
    }
    if release.prerelease {
        return Err(NotYet::Prerelease);
    }
    let (url, size, sha256) = select_asset(&release.assets, github_only)?;
    Ok((
        version,
        NativeRelease {
            version: format!("{major}.{minor}.{patch}"),
            url,
            sha256,
            size,
        },
    ))
}

/// `GET /repos/115jon/yap/releases?per_page=30`: the highest version among
/// the releases that pass [`accept_release`]. Not `releases/latest`: that
/// stays the Tauri release (the Tauri updater reads its `latest.json`), and
/// a later 3.14.x made latest must not hide the native app.
pub fn pick_native_release(json: &[u8]) -> Result<NativeRelease, NotYet> {
    let releases: Vec<GhRelease> =
        serde_json::from_slice(json).map_err(|e| NotYet::Unreadable(e.to_string()))?;
    releases
        .iter()
        .filter_map(|r| accept(r, true).ok())
        .max_by_key(|(version, _)| *version)
        .map(|(_, release)| release)
        .ok_or(NotYet::NoNativeRelease {
            seen: releases.len(),
        })
}

/// The test override file (`YAP_HANDOFF_RELEASE`, never read by an
/// installed copy): a GitHub release object as `release`, and optionally
/// the switch's answer as `switch` (otherwise the real endpoint is asked).
#[derive(Debug, Deserialize)]
struct OverrideFile {
    release: GhRelease,
    #[serde(default)]
    switch: Option<RemoteSwitch>,
}

/// What the override file holds once its release passed [`accept_release`].
#[derive(Debug)]
pub struct Override {
    pub release: Result<NativeRelease, NotYet>,
    pub switch: Option<RemoteSwitch>,
}

pub fn parse_override(json: &[u8]) -> Result<Override, String> {
    let file: OverrideFile = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    Ok(Override {
        release: accept(&file.release, false).map(|(_, r)| r),
        switch: file.switch,
    })
}

// ── The remote switch and the rollout ────────────────────────────────────

/// `GET /api/desktop/native-handoff` (src/routes/api/desktop/native-handoff.ts).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteSwitch {
    pub enabled: bool,
    pub rollout_percent: u8,
}

/// This install's stable bucket, 0–99: the SHA-256 of its id (a GUID kept
/// in the marker), first 8 bytes as a big-endian integer, mod 100.
pub fn rollout_bucket(install_id: &str) -> u8 {
    let digest = Sha256::digest(install_id.as_bytes());
    let mut first = [0u8; 8];
    first.copy_from_slice(&digest[..8]);
    (u64::from_be_bytes(first) % 100) as u8
}

/// Whether this install is in: the switch on and its bucket below the
/// rollout percent.
pub fn rollout_admits(switch: &RemoteSwitch, install_id: &str) -> bool {
    switch.enabled
        && switch.rollout_percent <= 100
        && rollout_bucket(install_id) < switch.rollout_percent
}

/// The switch endpoint's answer, as the bridge got it.
#[derive(Debug)]
pub enum SwitchAnswer<'a> {
    /// The request failed (network, TLS, timeout).
    Unreachable(String),
    /// An HTTP status and body.
    Response { status: u16, body: &'a [u8] },
}

/// Fails closed: only a 200 with a readable body that admits this install
/// lets the handoff go on. Everything else is "no", with the reason.
pub fn switch_admits(answer: SwitchAnswer<'_>, install_id: &str) -> Result<(), String> {
    match answer {
        SwitchAnswer::Unreachable(e) => Err(format!("switch unreachable: {e}")),
        SwitchAnswer::Response { status, .. } if status != 200 => {
            Err(format!("switch answered {status}"))
        }
        SwitchAnswer::Response { body, .. } => {
            let switch: RemoteSwitch =
                serde_json::from_slice(body).map_err(|e| format!("switch not readable: {e}"))?;
            admitted(&switch, install_id)
        }
    }
}

/// [`rollout_admits`] with the reason when it does not.
pub fn admitted(switch: &RemoteSwitch, install_id: &str) -> Result<(), String> {
    if !switch.enabled {
        return Err("switch is off".into());
    }
    if rollout_admits(switch, install_id) {
        Ok(())
    } else {
        Err(format!(
            "not in the rollout (bucket {} of {}%)",
            rollout_bucket(install_id),
            switch.rollout_percent
        ))
    }
}

// ── Verification ─────────────────────────────────────────────────────────

/// Whether a SHA-256 equals the release's digest (hex, any case).
pub fn digest_matches(expected_hex: &str, actual: &[u8]) -> bool {
    let actual: String = actual.iter().map(|b| format!("{b:02x}")).collect();
    actual.len() == 64 && expected_hex.eq_ignore_ascii_case(&actual)
}

/// The signer rule for the Setup and the installed `Yap.exe`: subject CN
/// exactly [`SIGNER_NAME`], an issuer CN starting with [`ISSUER_PREFIX`], a
/// timestamp, and both [`CODE_SIGNING_EKU`] and [`IDENTITY_EKU`].
pub fn check_signer(
    subject: &str,
    issuer: &str,
    timestamped: bool,
    ekus: &[String],
) -> Result<(), String> {
    if subject != SIGNER_NAME {
        return Err(format!("signed by {subject:?}, expected {SIGNER_NAME:?}"));
    }
    if !issuer.starts_with(ISSUER_PREFIX) {
        return Err(format!(
            "issued by {issuer:?}, expected {ISSUER_PREFIX:?}..."
        ));
    }
    if !timestamped {
        return Err("the signature has no timestamp".into());
    }
    for eku in [CODE_SIGNING_EKU, IDENTITY_EKU] {
        if !ekus.iter().any(|e| e == eku) {
            return Err(format!("the certificate lacks the EKU {eku}"));
        }
    }
    Ok(())
}

/// Whether this Windows build may run the native app. Unknown: no.
pub fn windows_build_supported(build: Option<u32>) -> bool {
    build.is_some_and(|b| b >= MIN_WINDOWS_BUILD)
}

// ── After the launch ─────────────────────────────────────────────────────

/// What the bridge makes of the started native app so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Liveness {
    /// Keep waiting.
    Waiting,
    /// It is up: hand over.
    Alive,
    /// It is not: count a failure and stay.
    Dead(String),
}

/// `exit`: the exit code of the started `current\Yap.exe`, if it has
/// exited (0 when it handed its launch to an instance already running).
/// `native_running`: a process from `current\Yap.exe` runs now. Only that
/// counts, and only at the end of [`LIVENESS_WINDOW`]: the native app no
/// longer uninstalls this one until it has stayed up longer than that
/// (desktop-native `yap_core::tauri::UninstallDeferral`), so nothing it
/// starts early is proof of life.
pub fn judge_liveness(elapsed: Duration, exit: Option<i32>, native_running: bool) -> Liveness {
    if let Some(code) = exit.filter(|&c| c != 0) {
        return Liveness::Dead(format!("the native app exited with {code}"));
    }
    if elapsed < LIVENESS_WINDOW {
        return Liveness::Waiting;
    }
    if native_running {
        Liveness::Alive
    } else {
        Liveness::Dead(format!(
            "the native app was not running {} s after it started",
            LIVENESS_WINDOW.as_secs()
        ))
    }
}

// ── The marker ───────────────────────────────────────────────────────────

/// `%APPDATA%\Yap\native-handoff.json`.
pub const MARKER_FILE: &str = "native-handoff.json";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarkerState {
    /// Not handed off yet (or every try so far failed).
    #[default]
    Pending,
    /// The native app was installed, started and stayed up.
    Done,
}

/// What started a check or an attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// The launch updater window, which installs what it finds by itself.
    Automatic,
    /// The main window (the update toast, Settings): the person clicks
    /// "Update now".
    Prompted,
    /// `--yap-native-handoff` (CI, support).
    Support,
}

/// Why the bridge does nothing for a trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dormant {
    OptOut,
    /// Handed off already: the launch never reinstalls by itself.
    Done,
    /// [`MAX_AUTO_ATTEMPTS`] failed: the launch no longer installs alone.
    Exhausted,
}

impl fmt::Display for Dormant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OptOut => "opted out (optOut in the marker)",
            Self::Done => "already handed off",
            Self::Exhausted => "automatic attempts used up",
        })
    }
}

/// What a launch does before any window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchAction {
    /// Start the installed native app, and exit once it runs.
    LaunchNative,
    /// Start this app as usual.
    Continue,
}

/// A discovered release with when it was found (Unix ms).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedRelease {
    #[serde(flatten)]
    pub release: NativeRelease,
    pub checked_at: u64,
}

/// A boolean written as `true`/`false` or as the strings `"true"`/`"false"`
/// (a hand-edited marker).
fn lenient_bool<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Loose {
        Bool(bool),
        Text(String),
    }
    match Loose::deserialize(d)? {
        Loose::Bool(b) => Ok(b),
        Loose::Text(t) if t.trim().eq_ignore_ascii_case("true") => Ok(true),
        Loose::Text(t) if t.trim().eq_ignore_ascii_case("false") => Ok(false),
        Loose::Text(t) => Err(serde::de::Error::custom(format!("not a boolean: {t:?}"))),
    }
}

/// The marker: the spec's fields (`state`, `version`, `attempts`,
/// `lastAttempt`, `lastError`, `installId`, `optOut`) plus the bridge's
/// bookkeeping. Times are Unix milliseconds. Missing or unknown fields are
/// fine; a field that is there but unreadable makes the whole file
/// unreadable ([`Marker::parse`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Marker {
    pub state: MarkerState,
    /// The native version last attempted or handed off to.
    pub version: Option<String>,
    /// Failed install or launch attempts for [`Marker::version`].
    pub attempts: u32,
    pub last_attempt: Option<u64>,
    pub last_error: Option<String>,
    /// A GUID, made once: the rollout bucket's input.
    pub install_id: String,
    /// Set by hand (support): the bridge never runs.
    #[serde(deserialize_with = "lenient_bool")]
    pub opt_out: bool,
    /// The download page was opened after the automatic attempts ran out.
    #[serde(deserialize_with = "lenient_bool")]
    pub download_page_shown: bool,
    /// Launches in a row whose redirect to the installed native app failed.
    pub redirect_failures: u32,
    /// No GitHub request before this (a "not yet" or an error backs off).
    pub next_check_at: Option<u64>,
    /// The last accepted release, so a launch can answer without GitHub.
    pub release: Option<CachedRelease>,
}

fn ms(d: Duration) -> u64 {
    d.as_millis() as u64
}

impl Marker {
    /// Reads a marker (a UTF-8 BOM is fine). Unreadable is an error: the
    /// caller then stays dormant and never rewrites the file.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
        if bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') {
            return Err(format!("{MARKER_FILE} is not a JSON object"));
        }
        serde_json::from_slice(bytes).map_err(|e| format!("{MARKER_FILE} is not readable: {e}"))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    /// Gives the marker an install id if it has none. True when it changed.
    pub fn ensure_install_id(&mut self, make: impl FnOnce() -> String) -> bool {
        if !self.install_id.trim().is_empty() {
            return false;
        }
        self.install_id = make();
        true
    }

    /// Whether `trigger` may check for and install the native app. Once
    /// done, only a person (or support) can start it again, for when the
    /// native app was removed since.
    pub fn may_run(&self, trigger: Trigger) -> Result<(), Dormant> {
        if self.opt_out {
            return Err(Dormant::OptOut);
        }
        if trigger == Trigger::Automatic {
            if self.state == MarkerState::Done {
                return Err(Dormant::Done);
            }
            if self.attempts >= MAX_AUTO_ATTEMPTS {
                return Err(Dormant::Exhausted);
            }
        }
        Ok(())
    }

    /// Before any window: hand straight to an installed native app once
    /// the handoff is done, unless that failed [`MAX_REDIRECT_FAILURES`]
    /// launches in a row.
    pub fn launch_action(&self, native_present: bool) -> LaunchAction {
        if !self.opt_out
            && self.state == MarkerState::Done
            && native_present
            && self.redirect_failures < MAX_REDIRECT_FAILURES
        {
            LaunchAction::LaunchNative
        } else {
            LaunchAction::Continue
        }
    }

    pub fn record_redirect(&mut self, ok: bool) {
        self.redirect_failures = if ok {
            0
        } else {
            self.redirect_failures.saturating_add(1)
        };
    }

    /// Whether GitHub may be asked at `now`.
    pub fn check_due(&self, now: u64) -> bool {
        !matches!(self.next_check_at, Some(at) if now < at)
    }

    pub fn back_off(&mut self, now: u64, wait: Duration) {
        self.next_check_at = Some(now.saturating_add(ms(wait)));
    }

    /// The cached release while it is fresh.
    pub fn fresh_release(&self, now: u64) -> Option<&NativeRelease> {
        self.release
            .as_ref()
            .filter(|c| now.saturating_sub(c.checked_at) < ms(RELEASE_CACHE_TTL))
            .map(|c| &c.release)
    }

    pub fn remember_release(&mut self, release: NativeRelease, now: u64) {
        self.release = Some(CachedRelease {
            release,
            checked_at: now,
        });
        self.next_check_at = None;
    }

    /// An attempt at `version` starts. A new version starts the count over
    /// (and may open the download page again).
    pub fn start_attempt(&mut self, version: &str, now: u64) {
        if self.version.as_deref() != Some(version) {
            self.attempts = 0;
            self.download_page_shown = false;
        }
        self.version = Some(version.to_owned());
        self.last_attempt = Some(now);
    }

    /// A failed attempt. `counted`: it failed installing or launching (not a
    /// refusal of the switch, nor a network or revocation error, which are
    /// only noted). True when this is the failure that uses up the automatic
    /// attempts and the download page should open now (once).
    pub fn record_failure(&mut self, error: &str, trigger: Trigger, counted: bool) -> bool {
        self.last_error = Some(error.to_owned());
        if !counted {
            return false;
        }
        self.attempts = self.attempts.saturating_add(1);
        let open = trigger != Trigger::Support
            && self.attempts >= MAX_AUTO_ATTEMPTS
            && !self.download_page_shown;
        if open {
            self.download_page_shown = true;
        }
        open
    }

    pub fn record_success(&mut self, version: &str) {
        self.state = MarkerState::Done;
        self.version = Some(version.to_owned());
        self.last_error = None;
        self.redirect_failures = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIGEST: &str = "sha256:6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d";

    fn release_json(tag: &str, draft: bool, prerelease: bool, assets: &str) -> Vec<u8> {
        format!(
            r#"{{"tag_name":"{tag}","draft":{draft},"prerelease":{prerelease},"name":"Yap {tag}","assets":[{assets}]}}"#
        )
        .into_bytes()
    }

    fn asset(name: &str, size: u64, digest: Option<&str>, url: Option<&str>) -> String {
        let url = url.map(str::to_owned).unwrap_or_else(|| {
            format!("https://github.com/115jon/yap/releases/download/v4.0.0/{name}")
        });
        let digest = digest.map_or("null".to_owned(), |d| format!("\"{d}\""));
        format!(
            r#"{{"name":"{name}","size":{size},"browser_download_url":"{url}","digest":{digest},"content_type":"application/octet-stream"}}"#
        )
    }

    fn setup_asset() -> String {
        asset(ASSET_NAME, 120_000_000, Some(DIGEST), None)
    }

    // ── Tags and versions ──

    #[test]
    fn tags_are_exactly_v_major_minor_patch() {
        assert_eq!(parse_tag("v4.0.0"), Some((4, 0, 0)));
        assert_eq!(parse_tag("v10.2.31"), Some((10, 2, 31)));
        for tag in [
            "4.0.0",
            "v4.0",
            "v4.0.0.1",
            "v4.0.0-beta.1",
            "v4.0.0+build",
            "V4.0.0",
            "desktop-native-v4.0.0",
            "v4..0",
            "v4.0.x",
            "v+4.0.0",
            "v 4.0.0",
            "v4.0.0 ",
            "",
            "v",
            "v9999999999.0.0",
        ] {
            assert_eq!(parse_tag(tag), None, "{tag:?}");
        }
    }

    #[test]
    fn a_v4_release_with_the_setup_is_accepted() {
        let json = release_json("v4.1.2", false, false, &setup_asset());
        let release = accept_release(&json, true).unwrap();
        assert_eq!(release.version, "4.1.2");
        assert_eq!(release.size, 120_000_000);
        assert_eq!(release.sha256, DIGEST.trim_start_matches("sha256:"));
        assert!(release.url.ends_with("/Yap.App-win-Setup.exe"));
    }

    #[test]
    fn a_tauri_release_is_not_yet() {
        for tag in ["v3.14.0", "v3.99.99", "v0.2.24"] {
            let json = release_json(tag, false, false, &setup_asset());
            assert!(matches!(
                accept_release(&json, true),
                Err(NotYet::OldMajor(_))
            ));
        }
        let json = release_json("desktop-native-v0.2.24", false, false, &setup_asset());
        assert!(matches!(accept_release(&json, true), Err(NotYet::Tag(_))));
    }

    #[test]
    fn drafts_and_prereleases_are_not_yet() {
        let draft = release_json("v4.0.0", true, false, &setup_asset());
        assert_eq!(accept_release(&draft, true), Err(NotYet::Draft));
        let pre = release_json("v4.0.0", false, true, &setup_asset());
        assert_eq!(accept_release(&pre, true), Err(NotYet::Prerelease));
    }

    #[test]
    fn an_unreadable_release_is_not_yet() {
        assert!(matches!(
            accept_release(b"<html>rate limited</html>", true),
            Err(NotYet::Unreadable(_))
        ));
        assert!(matches!(
            accept_release(br#"{"message":"Not Found"}"#, true),
            Err(NotYet::Unreadable(_))
        ));
    }

    // ── Discovery from the release list ──

    fn listed(tag: &str, draft: bool, prerelease: bool, assets: &str) -> String {
        String::from_utf8(release_json(tag, draft, prerelease, assets)).unwrap()
    }

    fn list(releases: &[String]) -> Vec<u8> {
        format!("[{}]", releases.join(",")).into_bytes()
    }

    #[test]
    fn the_highest_native_release_wins_whatever_is_latest() {
        let setup = setup_asset();
        // GitHub lists newest first: a 3.14.1 made latest after v4.
        let json = list(&[
            listed("v3.14.1", false, false, &setup),
            listed("v4.2.0", true, false, &setup),
            listed("v4.1.1", false, true, &setup),
            listed(
                "v4.1.0",
                false,
                false,
                &asset("Other.exe", 120_000_000, Some(DIGEST), None),
            ),
            listed("desktop-native-v0.2.30", false, false, &setup),
            listed("v4.0.10", false, false, &setup),
            listed("v4.0.9", false, false, &setup),
            listed("v3.14.0", false, false, &setup),
        ]);
        let release = pick_native_release(&json).unwrap();
        assert_eq!(release.version, "4.0.10", "numeric order, not text");
    }

    #[test]
    fn no_native_release_in_the_list_is_not_yet() {
        let setup = setup_asset();
        let json = list(&[
            listed("v3.14.0", false, false, &setup),
            listed("desktop-native-v0.2.25", false, false, &setup),
            listed("v4.0.0", true, false, &setup),
        ]);
        assert_eq!(
            pick_native_release(&json),
            Err(NotYet::NoNativeRelease { seen: 3 })
        );
        assert_eq!(
            pick_native_release(b"[]"),
            Err(NotYet::NoNativeRelease { seen: 0 })
        );
        for bad in [&br#"{"message":"API rate limit exceeded"}"#[..], b"<html/>"] {
            assert!(matches!(
                pick_native_release(bad),
                Err(NotYet::Unreadable(_))
            ));
        }
    }

    #[test]
    fn a_listed_release_needs_this_repositorys_asset_url() {
        let foreign = asset(
            ASSET_NAME,
            120_000_000,
            Some(DIGEST),
            Some("https://example.com/Yap.App-win-Setup.exe"),
        );
        let json = list(&[listed("v4.0.0", false, false, &foreign)]);
        assert!(pick_native_release(&json).is_err());
    }

    // ── Asset selection ──

    #[test]
    fn only_the_exact_asset_name_counts() {
        for name in [
            "yap.app-win-Setup.exe",
            "Yap.App-win-Setup.exe.blockmap",
            "Yap.App-beta-Setup.exe",
            "Yap.App-win-Setup.zip",
            "YapSetup.exe",
        ] {
            let json = release_json(
                "v4.0.0",
                false,
                false,
                &asset(name, 120_000_000, Some(DIGEST), None),
            );
            assert_eq!(accept_release(&json, true), Err(NotYet::NoAsset), "{name}");
        }
    }

    #[test]
    fn the_right_asset_is_picked_among_others() {
        let assets = [
            asset("Yap.App-win-Portable.zip", 130_000_000, Some(DIGEST), None),
            asset("releases.win.json", 2_000, Some(DIGEST), None),
            setup_asset(),
            asset("Yap.App-4.0.0-full.nupkg", 125_000_000, Some(DIGEST), None),
        ]
        .join(",");
        let json = release_json("v4.0.0", false, false, &assets);
        let release = accept_release(&json, true).unwrap();
        assert!(release.url.ends_with(ASSET_NAME));
        assert_eq!(release.size, 120_000_000);
    }

    #[test]
    fn the_asset_needs_a_sha256_digest() {
        for digest in [
            None,
            Some("sha1:da39a3ee5e6b4b0d3255bfef95601890afd80709"),
            Some("sha256:abc"),
            Some("sha256:zz340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d"),
            Some("6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d"),
        ] {
            let json = release_json(
                "v4.0.0",
                false,
                false,
                &asset(ASSET_NAME, 120_000_000, digest, None),
            );
            assert_eq!(
                accept_release(&json, true),
                Err(NotYet::NoDigest),
                "{digest:?}"
            );
        }
        assert_eq!(
            parse_digest("sha256:6E340B9CFFB37A989CA544E6BB780A2C78901D3FB33738768511A30617AFA01D")
                .as_deref(),
            Some("6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d")
        );
    }

    #[test]
    fn the_asset_size_is_10_to_500_mb() {
        for (size, ok) in [
            (9_999_999, false),
            (10_000_000, true),
            (500_000_000, true),
            (500_000_001, false),
            (0, false),
        ] {
            let json = release_json(
                "v4.0.0",
                false,
                false,
                &asset(ASSET_NAME, size, Some(DIGEST), None),
            );
            assert_eq!(accept_release(&json, true).is_ok(), ok, "{size}");
        }
    }

    #[test]
    fn a_discovered_asset_must_come_from_this_repository() {
        for url in [
            "https://github.com/someone/else/releases/download/v4.0.0/Yap.App-win-Setup.exe",
            "http://github.com/115jon/yap/releases/download/v4.0.0/Yap.App-win-Setup.exe",
            "https://example.com/Yap.App-win-Setup.exe",
        ] {
            let json = release_json(
                "v4.0.0",
                false,
                false,
                &asset(ASSET_NAME, 120_000_000, Some(DIGEST), Some(url)),
            );
            assert!(
                matches!(accept_release(&json, true), Err(NotYet::Url(_))),
                "{url}"
            );
        }
    }

    #[test]
    fn the_override_replaces_discovery_but_not_the_rules() {
        let local = asset(
            ASSET_NAME,
            120_000_000,
            Some(DIGEST),
            Some("http://127.0.0.1:8123/Yap.App-win-Setup.exe"),
        );
        let file = format!(
            r#"{{"release":{},"switch":{{"enabled":true,"rolloutPercent":100}}}}"#,
            String::from_utf8(release_json("v4.0.0", false, false, &local)).unwrap()
        );
        let parsed = parse_override(file.as_bytes()).unwrap();
        assert_eq!(
            parsed.release.unwrap().url,
            "http://127.0.0.1:8123/Yap.App-win-Setup.exe"
        );
        assert_eq!(
            parsed.switch,
            Some(RemoteSwitch {
                enabled: true,
                rollout_percent: 100
            })
        );
        // Still no draft, still a digest.
        let draft = format!(
            r#"{{"release":{}}}"#,
            String::from_utf8(release_json("v4.0.0", true, false, &local)).unwrap()
        );
        let parsed = parse_override(draft.as_bytes()).unwrap();
        assert_eq!(parsed.release, Err(NotYet::Draft));
        assert!(parsed.switch.is_none());
    }

    // ── Digest ──

    #[test]
    fn the_digest_must_match() {
        let data = b"the real setup";
        let actual = Sha256::digest(data);
        let hex: String = actual.iter().map(|b| format!("{b:02x}")).collect();
        assert!(digest_matches(&hex, &actual));
        assert!(digest_matches(&hex.to_ascii_uppercase(), &actual));
        let tampered = Sha256::digest(b"the real setup, changed");
        assert!(!digest_matches(&hex, &tampered));
        assert!(!digest_matches("", &actual));
        assert!(!digest_matches(&hex, &actual[..31]));
    }

    // ── Signer and issuer ──

    fn artifact_ekus() -> Vec<String> {
        // As read off the native 0.2.25 Setup's certificate.
        vec![
            "1.3.6.1.4.1.311.97.1.0".into(),
            CODE_SIGNING_EKU.into(),
            IDENTITY_EKU.into(),
        ]
    }

    #[test]
    fn the_signer_issuer_timestamp_and_ekus_must_all_hold() {
        let ekus = artifact_ekus();
        for issuer in [
            "Microsoft ID Verified CS AOC CA 03",
            "Microsoft ID Verified CS AOC CA 04",
            "Microsoft ID Verified CS EOC CA 01",
        ] {
            assert!(check_signer("Jonathan Martinez", issuer, true, &ekus).is_ok());
        }
        let issuer = "Microsoft ID Verified CS AOC CA 04";
        for (subject, issuer, ts) in [
            ("Jonathan Martinez", issuer, false),
            ("jonathan martinez", issuer, true),
            ("Jonathan Martinez ", issuer, true),
            ("Jonathan Martinez Evil", issuer, true),
            ("OBS Project, LLC", issuer, true),
            (
                "Jonathan Martinez",
                "DigiCert Trusted G4 Code Signing",
                true,
            ),
            ("Jonathan Martinez", "Evil Microsoft ID Verified CS", true),
            ("Jonathan Martinez", "", true),
            ("", issuer, true),
        ] {
            assert!(
                check_signer(subject, issuer, ts, &ekus).is_err(),
                "{subject:?} {issuer:?} {ts}"
            );
        }
    }

    #[test]
    fn another_artifact_signing_identity_is_refused() {
        let issuer = "Microsoft ID Verified CS AOC CA 04";
        // Someone else's validation: generic public-trust OID, their own id.
        let other: Vec<String> = vec![
            "1.3.6.1.4.1.311.97.1.0".into(),
            CODE_SIGNING_EKU.into(),
            "1.3.6.1.4.1.311.97.111111111.222222222.333333333.44444444".into(),
        ];
        let err = check_signer("Jonathan Martinez", issuer, true, &other).unwrap_err();
        assert!(err.contains(IDENTITY_EKU), "{err}");
        let no_code_signing: Vec<String> = vec![IDENTITY_EKU.into()];
        assert!(check_signer("Jonathan Martinez", issuer, true, &no_code_signing).is_err());
        assert!(check_signer("Jonathan Martinez", issuer, true, &[]).is_err());
    }

    // ── Rollout and the switch ──

    #[test]
    fn the_bucket_is_stable_and_spread() {
        let id = "8F2B1C7A-3E4D-4F5A-9B6C-0D1E2F3A4B5C";
        assert_eq!(rollout_bucket(id), rollout_bucket(id));
        let mut seen = [0u32; 100];
        for i in 0..10_000 {
            seen[rollout_bucket(&format!("install-{i}")) as usize] += 1;
        }
        assert!(seen.iter().all(|&n| (50..=150).contains(&n)), "{seen:?}");
    }

    #[test]
    fn the_rollout_admits_buckets_below_the_percent() {
        let on = |p| RemoteSwitch {
            enabled: true,
            rollout_percent: p,
        };
        let ids: Vec<String> = (0..2_000).map(|i| format!("install-{i}")).collect();
        assert!(ids.iter().all(|id| !rollout_admits(&on(0), id)));
        assert!(ids.iter().all(|id| rollout_admits(&on(100), id)));
        for id in &ids {
            let bucket = rollout_bucket(id);
            assert!(!rollout_admits(&on(bucket), id));
            assert!(rollout_admits(&on(bucket + 1), id));
        }
        // Raising the percent only adds installs.
        for id in &ids {
            if rollout_admits(&on(25), id) {
                assert!(rollout_admits(&on(50), id));
            }
        }
        let off = RemoteSwitch {
            enabled: false,
            rollout_percent: 100,
        };
        assert!(ids.iter().all(|id| !rollout_admits(&off, id)));
        let silly = on(250);
        assert!(ids.iter().all(|id| !rollout_admits(&silly, id)));
    }

    #[test]
    fn the_kill_switch_fails_closed() {
        let id = "install-1";
        let all = br#"{"enabled":true,"rolloutPercent":100}"#;
        assert!(switch_admits(
            SwitchAnswer::Response {
                status: 200,
                body: all
            },
            id
        )
        .is_ok());
        let refused: [(u16, &[u8]); 11] = [
            (404, all),
            (500, all),
            (503, b""),
            (200, b""),
            (200, b"<html>Cloudflare</html>"),
            (200, br#"{"enabled":"true","rolloutPercent":100}"#),
            (200, br#"{"enabled":true}"#),
            (200, br#"{"enabled":true,"rolloutPercent":-1}"#),
            (200, br#"{"enabled":true,"rolloutPercent":100,"extra":1}"#),
            (200, br#"{"enabled":false,"rolloutPercent":100}"#),
            (200, br#"{"enabled":true,"rolloutPercent":0}"#),
        ];
        for (status, body) in refused {
            assert!(
                switch_admits(SwitchAnswer::Response { status, body }, id).is_err(),
                "{status} {}",
                String::from_utf8_lossy(body)
            );
        }
        for e in ["timed out", "dns error"] {
            assert!(switch_admits(SwitchAnswer::Unreachable(e.into()), id).is_err());
        }
    }

    // ── Windows build ──

    #[test]
    fn windows_10_1809_and_later_only() {
        assert!(!windows_build_supported(None));
        assert!(!windows_build_supported(Some(17134)));
        assert!(windows_build_supported(Some(17763)));
        assert!(windows_build_supported(Some(19045)));
        assert!(windows_build_supported(Some(26200)));
    }

    // ── Liveness ──

    #[test]
    fn the_native_app_must_still_run_at_the_end_of_8_seconds() {
        let s = Duration::from_secs;
        assert_eq!(judge_liveness(s(1), None, false), Liveness::Waiting);
        // Running early is not enough: only the end of the window counts.
        assert_eq!(judge_liveness(s(7), None, true), Liveness::Waiting);
        assert_eq!(judge_liveness(s(7), Some(0), true), Liveness::Waiting);
        assert_eq!(judge_liveness(s(8), None, true), Liveness::Alive);
        // Exit 0 after handing to a running instance, which still runs.
        assert_eq!(judge_liveness(s(8), Some(0), true), Liveness::Alive);
        // Up at first, gone by the end.
        assert!(matches!(
            judge_liveness(s(8), Some(0), false),
            Liveness::Dead(_)
        ));
        assert!(matches!(
            judge_liveness(s(9), None, false),
            Liveness::Dead(_)
        ));
        // A crash fails at once.
        assert!(matches!(
            judge_liveness(s(1), Some(-1073741819), false),
            Liveness::Dead(_)
        ));
    }

    // ── The marker ──

    #[test]
    fn the_marker_round_trips_with_the_spec_field_names() {
        let mut marker = Marker::default();
        assert!(marker.ensure_install_id(|| "GUID-1".into()));
        assert!(!marker.ensure_install_id(|| "GUID-2".into()));
        marker.start_attempt("4.0.0", 1_000);
        marker.record_failure("Setup exited with 1", Trigger::Automatic, true);
        let json = marker.to_json();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["state"], "pending");
        assert_eq!(value["version"], "4.0.0");
        assert_eq!(value["attempts"], 1);
        assert_eq!(value["lastAttempt"], 1_000);
        assert_eq!(value["lastError"], "Setup exited with 1");
        assert_eq!(value["installId"], "GUID-1");
        assert_eq!(value["optOut"], false);
        assert_eq!(Marker::parse(json.as_bytes()), Ok(marker));
    }

    #[test]
    fn a_hand_written_marker_reads() {
        let m = Marker::parse(br#"{"optOut":true}"#).unwrap();
        assert!(m.opt_out);
        assert_eq!(m.state, MarkerState::Pending);
        let m = Marker::parse(br#"{"state":"done","installId":"x","futureField":[1]}"#).unwrap();
        assert_eq!(m.state, MarkerState::Done);
        // Notepad's BOM, and booleans as strings.
        let m = Marker::parse(b"\xEF\xBB\xBF{\"optOut\":\"true\"}").unwrap();
        assert!(m.opt_out);
        let m = Marker::parse(br#"{"optOut":" FALSE "}"#).unwrap();
        assert!(!m.opt_out);
    }

    #[test]
    fn an_unreadable_marker_is_an_error_not_a_fresh_start() {
        for bytes in [
            &b"not json"[..],
            b"",
            b"\xEF\xBB\xBF",
            br#"{"optOut":"yes"}"#,
            br#"{"optOut":1}"#,
            br#"{"state":"finished"}"#,
            br#"{"attempts":"three"}"#,
            br#"[]"#,
        ] {
            assert!(
                Marker::parse(bytes).is_err(),
                "{}",
                String::from_utf8_lossy(bytes)
            );
        }
    }

    #[test]
    fn three_failed_attempts_stop_the_automatic_ones_and_open_the_page_once() {
        let mut m = Marker::default();
        m.start_attempt("4.0.0", 1);
        assert_eq!(m.may_run(Trigger::Automatic), Ok(()));
        assert!(!m.record_failure("one", Trigger::Automatic, true));
        assert!(!m.record_failure("two", Trigger::Automatic, true));
        assert_eq!(m.may_run(Trigger::Automatic), Ok(()));
        assert!(
            m.record_failure("three", Trigger::Automatic, true),
            "page opens"
        );
        assert_eq!(m.attempts, 3);
        assert_eq!(m.may_run(Trigger::Automatic), Err(Dormant::Exhausted));
        // A manual retry may still run, and failing again opens nothing.
        assert_eq!(m.may_run(Trigger::Prompted), Ok(()));
        assert_eq!(m.may_run(Trigger::Support), Ok(()));
        assert!(!m.record_failure("four", Trigger::Prompted, true));
        assert_eq!(m.last_error.as_deref(), Some("four"));
        assert_eq!(m.state, MarkerState::Pending);
    }

    #[test]
    fn refusals_and_network_errors_are_noted_but_not_counted() {
        let mut m = Marker::default();
        m.start_attempt("4.0.0", 1);
        for _ in 0..10 {
            assert!(!m.record_failure("switch is off", Trigger::Automatic, false));
            assert!(!m.record_failure("download: timed out", Trigger::Automatic, false));
        }
        assert_eq!(m.attempts, 0);
        assert_eq!(m.last_error.as_deref(), Some("download: timed out"));
        assert_eq!(m.may_run(Trigger::Automatic), Ok(()));
    }

    #[test]
    fn a_new_version_starts_the_count_over() {
        let mut m = Marker::default();
        m.start_attempt("4.0.0", 1);
        for _ in 0..3 {
            m.record_failure("Setup exited with 1", Trigger::Automatic, true);
        }
        assert_eq!(m.may_run(Trigger::Automatic), Err(Dormant::Exhausted));
        m.start_attempt("4.0.0", 2);
        assert_eq!(m.attempts, 3, "the same version keeps its count");
        m.start_attempt("4.0.1", 3);
        assert_eq!(m.attempts, 0);
        assert!(!m.download_page_shown);
        assert_eq!(m.may_run(Trigger::Automatic), Ok(()));
    }

    #[test]
    fn support_runs_never_open_the_page() {
        let mut m = Marker::default();
        for _ in 0..5 {
            assert!(!m.record_failure("ci", Trigger::Support, true));
        }
        assert!(!m.download_page_shown);
    }

    #[test]
    fn done_redirects_to_native_and_only_a_person_reinstalls() {
        let mut m = Marker::default();
        m.record_failure("one", Trigger::Automatic, true);
        m.record_success("4.0.0");
        assert_eq!(m.state, MarkerState::Done);
        assert_eq!(m.last_error, None);
        assert_eq!(m.launch_action(true), LaunchAction::LaunchNative);
        // Native removed since: the launch stays and never reinstalls alone,
        // but the toast or Settings may offer it again.
        assert_eq!(m.launch_action(false), LaunchAction::Continue);
        assert_eq!(m.may_run(Trigger::Automatic), Err(Dormant::Done));
        assert_eq!(m.may_run(Trigger::Prompted), Ok(()));
        assert_eq!(m.may_run(Trigger::Support), Ok(()));
        // Pending never skips the app.
        assert_eq!(
            Marker::default().launch_action(true),
            LaunchAction::Continue
        );
    }

    #[test]
    fn two_failed_redirects_in_a_row_start_tauri_instead() {
        let mut m = Marker::default();
        m.record_success("4.0.0");
        m.record_redirect(false);
        assert_eq!(m.launch_action(true), LaunchAction::LaunchNative);
        m.record_redirect(true);
        assert_eq!(m.redirect_failures, 0, "a good one resets the run");
        m.record_redirect(false);
        m.record_redirect(false);
        assert_eq!(m.launch_action(true), LaunchAction::Continue);
        // A handoff that works again re-arms it.
        m.record_success("4.0.1");
        assert_eq!(m.launch_action(true), LaunchAction::LaunchNative);
    }

    #[test]
    fn opting_out_stops_everything() {
        let mut m = Marker {
            opt_out: true,
            ..Marker::default()
        };
        for t in [Trigger::Automatic, Trigger::Prompted, Trigger::Support] {
            assert_eq!(m.may_run(t), Err(Dormant::OptOut));
        }
        m.record_success("4.0.0");
        assert_eq!(m.launch_action(true), LaunchAction::Continue);
    }

    #[test]
    fn github_is_asked_at_most_once_per_backoff() {
        let mut m = Marker::default();
        assert!(m.check_due(0));
        m.back_off(1_000, NOT_YET_BACKOFF);
        assert!(!m.check_due(1_000 + ms(NOT_YET_BACKOFF) - 1));
        assert!(m.check_due(1_000 + ms(NOT_YET_BACKOFF)));
    }

    #[test]
    fn a_found_release_is_reused_while_fresh() {
        let mut m = Marker::default();
        m.back_off(0, ERROR_BACKOFF);
        let release = NativeRelease {
            version: "4.0.0".into(),
            url: "https://github.com/115jon/yap/releases/download/v4.0.0/Yap.App-win-Setup.exe"
                .into(),
            sha256: "ab".repeat(32),
            size: 120_000_000,
        };
        m.remember_release(release.clone(), 10);
        assert!(m.check_due(10), "a find clears the backoff");
        assert_eq!(m.fresh_release(10), Some(&release));
        assert_eq!(m.fresh_release(10 + ms(RELEASE_CACHE_TTL)), None);
        let reread = Marker::parse(m.to_json().as_bytes()).unwrap();
        assert_eq!(reread.fresh_release(11), Some(&release));
    }
}

#[cfg(test)]
mod real_list {
    use super::*;

    /// GitHub's real `/releases?per_page=30` answer, when given
    /// (`YAP_HANDOFF_TEST_RELEASES=<file>`): it must parse, and only a v4.x.y
    /// release may be picked from it.
    #[test]
    fn the_real_release_list_parses_when_given() {
        let Some(path) = std::env::var_os("YAP_HANDOFF_TEST_RELEASES") else {
            eprintln!("skipped: YAP_HANDOFF_TEST_RELEASES is not set");
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        match pick_native_release(&bytes) {
            Ok(release) => assert!(release.version.starts_with(|c: char| c >= '4')),
            Err(NotYet::NoNativeRelease { seen }) => {
                eprintln!("no native release among {seen}");
                assert!(seen > 0);
            }
            Err(other) => panic!("{other}"),
        }
    }
}
