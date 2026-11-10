//! Deadlock's live state ("In match as Haze, ranked, with 2 friends") from
//! the game's own `console.log`, which it writes when started with
//! `-condebug`, for op 43's `game_activity.game`
//! (`packages/protocol/src/game-activity.ts` `DeadlockLiveState`).
//!
//! - `locate`: the log's path, from Steam's libraries.
//! - `tail`: only the bytes added since the last read, bounded, starting
//!   over when the game recreates the file.
//! - `parse`: the lines into phase, hero, mode, phase start, account, party.
//! - `throttle`: at most 10 updates a minute (the gateway takes 12).
//! - `watch`: one thread doing that once a second while the detector says
//!   Deadlock runs, and never otherwise.
//!
//! It only ever reads that one file, opened read-only. It never opens the
//! game's process, reads its memory, injects anything or writes to any
//! game file; the account and party ids it reads go only to the gateway,
//! which checks them against the linked Steam account and never passes
//! them on.

pub mod locate;
pub mod parse;
pub mod tail;
pub mod throttle;
pub mod watch;

use yap_protocol::game_activity::{is_game_image_path, DeadlockLiveState, GameActivity};

use crate::Detected;
use parse::{LogSecs, Snapshot, YEAR_SECS};

/// Deadlock on Steam.
pub const STEAM_APP_ID: u32 = 1_422_450;
/// Deadlock in the game catalog (its Discord application id).
pub const CATALOG_ID: &str = "1276737795012165766";
/// The launch option that makes the game write `console.log`.
pub const LAUNCH_OPTION: &str = "-condebug";
/// Steam's `run/<app>/<language>/<arguments>/`: Deadlock once with the
/// option (Steam asks before it passes arguments from a link).
pub const LAUNCH_URL: &str = "steam://run/1422450//-condebug/";

/// Whether a detection is Deadlock.
pub fn is_deadlock(detected: &Detected) -> bool {
    detected.app_id.as_deref() == Some(CATALOG_ID)
        || detected.steam_app_id.as_deref() == Some("1422450")
}

/// The hero's small portrait on our own asset route, when the key makes a
/// valid path.
pub fn hero_image(hero: &str) -> Option<String> {
    let path = format!("/api/game-assets/{CATALOG_ID}/hero_{hero}_sm.png");
    is_game_image_path(&path).then_some(path)
}

/// The activity with the live state, and the hero's portrait as its large
/// image.
pub fn with_live(mut activity: GameActivity, live: Option<&DeadlockLiveState>) -> GameActivity {
    if let Some(live) = live.filter(|live| live.is_valid()) {
        activity.large_image = live.hero.as_deref().and_then(hero_image);
        activity.game = Some(live.clone());
    }
    activity
}

/// Epoch ms of a log time, from one known pair (`anchor` on the log's clock
/// is `anchor_ms`). `None` for a time after the anchor or more than two
/// days before it (the log has no year; anything that old is a guess).
pub fn log_epoch_ms(at: LogSecs, anchor: LogSecs, anchor_ms: i64) -> Option<i64> {
    let back = if at <= anchor {
        anchor - at
    } else {
        // Across New Year.
        (anchor + YEAR_SECS).checked_sub(at)?
    };
    (back <= 2 * 86_400).then(|| (anchor_ms - i64::from(back) * 1000).max(0))
}

/// A snapshot as the protocol has it, with the phase's start already in
/// epoch ms.
pub fn live_state(snapshot: &Snapshot, phase_started_at: Option<i64>) -> DeadlockLiveState {
    let mut live = DeadlockLiveState::new(snapshot.phase.as_str());
    live.hero = snapshot.hero.clone();
    live.mode = snapshot.mode.map(|mode| mode.as_str().to_owned());
    live.phase_started_at = phase_started_at;
    live.account_id = snapshot.account_id;
    live.party = (!snapshot.party.is_empty()).then(|| snapshot.party.clone());
    live
}

#[cfg(test)]
mod tests;
