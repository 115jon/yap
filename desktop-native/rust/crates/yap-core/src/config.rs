//! Where the client talks to. Every value can be overridden by an env var so
//! the same binary can point at a local worker.
//!
//! The defaults mirror `src/lib/site-defaults.ts` in the web app; keep them in
//! step with it (it is the source of truth for hostnames).

use std::sync::OnceLock;

pub struct Config {
    /// Yap API + gateway origin, e.g. `https://yap.115jon.com`.
    pub api_base: String,
    /// Kova auth server, e.g. `https://auth.115jon.com`.
    pub kova_auth_url: String,
    /// Yap's Kova publishable key.
    pub publishable_key: String,
    /// `Origin` sent on realtime WebSockets. The worker admits its own
    /// origin (`isAllowedRealtimeOrigin`), so this defaults to `api_base`.
    pub gateway_origin: String,
    /// A session token to use instead of the credential store (dev only).
    pub dev_token: Option<String>,
    /// Hidden standby sessions on occupied voice landings, as on the web;
    /// `YAP_VOICE_STANDBY=off` turns them off.
    pub voice_standby: bool,
    /// Our game activity (op 43) may go to a gateway that takes it;
    /// `YAP_GAME_ACTIVITY_SEND=off` never sends it. Either way it is sent
    /// only to a server whose presence carries `game_activity`.
    pub activity_send: bool,
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_owned())
}

pub fn get() -> &'static Config {
    static CONFIG: OnceLock<Config> = OnceLock::new();
    CONFIG.get_or_init(|| {
        let api_base = env_or("YAP_API_URL", "https://yap.115jon.com")
            .trim_end_matches('/')
            .to_owned();
        Config {
            gateway_origin: env_or("YAP_GATEWAY_ORIGIN", &api_base),
            api_base,
            kova_auth_url: env_or("YAP_KOVA_AUTH_URL", "https://auth.115jon.com")
                .trim_end_matches('/')
                .to_owned(),
            publishable_key: env_or(
                "YAP_KOVA_PUBLISHABLE_KEY",
                "pk_live_yl2Z9ssEwlWuOYHVRWozk9WThozu4nFP3QAhCWAsPhE",
            ),
            dev_token: std::env::var("YAP_TOKEN").ok().filter(|v| !v.is_empty()),
            voice_standby: !std::env::var("YAP_VOICE_STANDBY")
                .is_ok_and(|v| v.trim().eq_ignore_ascii_case("off")),
            activity_send: !std::env::var("YAP_GAME_ACTIVITY_SEND")
                .is_ok_and(|v| v.trim().eq_ignore_ascii_case("off")),
        }
    })
}
