//! The recorded fixtures (`fixtures/deadlock`, written from the line shapes
//! a 2026 log has, with made-up ids and names) through the parser, and the
//! follower over a file that grows, is truncated and is recreated.

use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

use super::parse::{line_time, Mode, Parser, Phase, Snapshot};
use super::watch::{system_ms, Follower, LogStatus, WatchConfig, GRACE};
use super::*;

const SESSION: &str = include_str!("../../fixtures/deadlock/session.txt");
const MODES: &str = include_str!("../../fixtures/deadlock/modes.txt");

const ME: u32 = 1_000_001;
const FRIEND: u32 = 2_000_002;
const OTHER: u32 = 3_000_003;

fn at(time: &str) -> u32 {
    line_time(&format!("{time} x")).expect("a time").0
}

/// Every distinct snapshot, in order, with the line that made it.
fn transitions(log: &str) -> Vec<(String, Snapshot)> {
    let mut parser = Parser::new();
    let mut seen: Vec<(String, Snapshot)> = Vec::new();
    for line in log.lines() {
        parser.feed(line);
        let snapshot = parser.snapshot();
        if seen.last().is_none_or(|(_, last)| *last != snapshot) {
            seen.push((line.trim().to_owned(), snapshot));
        }
    }
    seen
}

/// The snapshot right after the first line containing `marker`.
fn after(log: &str, marker: &str) -> Snapshot {
    let mut parser = Parser::new();
    for line in log.lines() {
        parser.feed(line);
        if line.contains(marker) {
            return parser.snapshot();
        }
    }
    panic!("no line has {marker:?}");
}

fn phases(log: &str) -> Vec<Phase> {
    let mut list: Vec<Phase> = Vec::new();
    for (_, snapshot) in transitions(log) {
        if list.last() != Some(&snapshot.phase) {
            list.push(snapshot.phase);
        }
    }
    list
}

#[test]
fn a_session_moves_through_every_phase_in_order() {
    use Phase::*;
    assert_eq!(
        phases(SESSION),
        [Menu, Hideout, Queue, MatchIntro, InMatch, PostMatch, Hideout, Menu]
    );
}

#[test]
fn each_phase_keeps_the_time_it_began() {
    let since = |marker| after(SESSION, marker).phase_since;
    assert_eq!(since("Creating device"), Some(at("11/09 20:00:00")));
    assert_eq!(since("[Client] Map: \"dl_hideout\""), Some(at("11/09 20:01:00")));
    // A later line in the same phase leaves it.
    assert_eq!(since("STEAM USERID validated"), Some(at("11/09 20:01:00")));
    assert_eq!(since("Entering matchmaking"), Some(at("11/09 20:04:00")));
    assert_eq!(since("OnMatchFormed"), Some(at("11/09 20:06:49")));
    assert_eq!(since("20:07:37 OnGameStateChanged"), Some(at("11/09 20:07:37")));
    assert_eq!(since("PostGame (8)"), Some(at("11/09 20:49:31")));
    assert_eq!(since("End (11)"), Some(at("11/09 20:49:31")));
}

#[test]
fn the_account_is_the_local_servers_check_of_the_player() {
    assert_eq!(after(SESSION, "Creating device").account_id, None);
    assert_eq!(after(SESSION, "STEAM USERID validated").account_id, Some(ME));
    assert_eq!(after(SESSION, "PostGame (8)").account_id, Some(ME));
}

#[test]
fn the_hero_follows_the_hideout_and_locks_in_a_match() {
    assert_eq!(after(SESSION, "heroes_wip/vampirebat").hero.as_deref(), Some("vampirebat"));
    // Swapped in the Hideout: the newest camera wins.
    assert_eq!(after(SESSION, "Loaded hero 279/hero_haze").hero.as_deref(), Some("vampirebat"));
    let swapped = after(SESSION, "[Client] Players: 2 (3 bots)");
    assert_eq!(swapped.hero.as_deref(), Some("haze"));
    // Queued: still the Hideout's hero.
    assert_eq!(after(SESSION, "Entering matchmaking").hero.as_deref(), Some("haze"));
    // A match formed: nobody yet, then the first camera, then kill cams
    // and spectated teammates never replace it.
    assert_eq!(after(SESSION, "OnMatchFormed").hero, None);
    assert_eq!(after(SESSION, "heroes_staging/warden").hero.as_deref(), Some("haze"));
    assert_eq!(after(SESSION, "heroes_staging/viper").hero.as_deref(), Some("haze"));
    // Post match keeps the match's hero; the menu shows none.
    assert_eq!(after(SESSION, "PostGame (8)").hero.as_deref(), Some("haze"));
    assert_eq!(after(SESSION, "Source2Shutdown").hero, None);
}

#[test]
fn the_party_follows_joins_and_leaves() {
    assert!(after(SESSION, "NoLobby (0)").party.is_empty());
    assert_eq!(after(SESSION, "initiator_account_id: 2000002 }").party, [FRIEND]);
    assert_eq!(after(SESSION, "initiator_account_id: 3000003 }").party, [FRIEND, OTHER]);
    // Other party events change nothing.
    assert_eq!(after(SESSION, "k_eDraftEnded_User").party, [FRIEND, OTHER]);
    assert_eq!(after(SESSION, "k_eLeftParty initiator_account_id: 3000003").party, [FRIEND]);
    // Kept through the match.
    assert_eq!(after(SESSION, "PostGame (8)").party, [FRIEND]);
    // We left: nobody.
    assert!(after(SESSION, "k_eLeftParty initiator_account_id: 1000001").party.is_empty());
}

#[test]
fn joining_someone_elses_party_starts_a_new_list_without_us() {
    let mut parser = Parser::new();
    for line in [
        "11/09 20:00:01 [Server] SV:  \"P<0><[U:1:1000001]><>\" STEAM USERID validated",
        "11/09 20:00:02 [Client] Map: \"dl_hideout\"",
        "11/09 20:01:00 CMsgGCToClientPartyEvent: { party_id: 1 event: k_eJoinedParty initiator_account_id: 2000002 }",
        "11/09 20:02:00 CMsgGCToClientPartyEvent: { party_id: 9 event: k_eJoinedParty initiator_account_id: 1000001 }",
        "11/09 20:02:05 CMsgGCToClientPartyEvent: { party_id: 9 event: k_eJoinedParty initiator_account_id: 4000004 }",
    ] {
        parser.feed(line);
    }
    assert_eq!(parser.snapshot().party, [4_000_004]);
    parser.feed("11/09 20:03:00 CMsgGCToClientPartyEvent: { party_id: 9 event: k_ePartyDisbanded initiator_account_id: 4000004 }");
    assert!(parser.snapshot().party.is_empty());
}

#[test]
fn a_party_names_at_most_five_others_and_never_us() {
    let mut parser = Parser::new();
    parser.feed("11/09 20:00:01 [Server] SV:  \"P<0><[U:1:1000001]><>\" STEAM USERID validated");
    for id in [1_000_001, 11, 12, 13, 14, 15, 16, 17] {
        parser.feed(&format!(
            "11/09 20:01:00 CMsgGCToClientPartyEvent: {{ party_id: 3 event: k_eJoinedParty initiator_account_id: {id} }}"
        ));
    }
    let party = parser.snapshot().party;
    assert_eq!(party.len(), 5);
    assert!(!party.contains(&ME));
    assert_eq!(party.last(), Some(&17));
}

#[test]
fn the_queue_ends_with_a_stop_or_a_match() {
    let stopped = after(MODES, "k_EMsgClientToGCStopMatchmaking");
    assert_eq!(stopped.phase, Phase::Hideout);
    assert_eq!(after(MODES, "21:01:20 Entering matchmaking").phase, Phase::Queue);
    assert_eq!(after(MODES, "Lobby 1 for Match 2 created").phase, Phase::MatchIntro);
}

#[test]
fn modes_from_counts_bots_names_and_maps() {
    // Eight players: Street Brawl, known once the match runs.
    assert_eq!(after(MODES, "Players: 8 (0 bots)").mode, None);
    let brawl = after(MODES, "21:02:20 OnGameStateChanged: GameInProgress");
    assert_eq!((brawl.phase, brawl.mode), (Phase::InMatch, Some(Mode::Brawl)));
    assert_eq!(brawl.hero.as_deref(), Some("bebop"));
    // Mostly bots.
    let bots = after(MODES, "21:13:20 OnGameStateChanged: GameInProgress");
    assert_eq!(bots.mode, Some(Mode::Bots));
    assert_eq!(bots.hero.as_deref(), Some("tengu"));
    // Leaving a match early is its end.
    assert_eq!(after(MODES, "21:30:00").phase, Phase::PostMatch);
    // The game named it.
    let private = after(MODES, "21:31:20 OnGameStateChanged");
    assert_eq!(private.mode, Some(Mode::Private));
    // A full lobby with no bots says nothing of ranked or not.
    assert_eq!(after(SESSION, "20:07:37 OnGameStateChanged").mode, None);
    // Spectating shows no hero or mode, and ends in the menu.
    let watching = after(MODES, "heroes_staging/viper");
    assert_eq!((watching.phase, watching.hero.clone()), (Phase::Spectating, None));
    assert_eq!(after(MODES, "21:45:00").phase, Phase::Menu);
    // The practice map on this computer.
    let practice = after(MODES, "21:45:03");
    assert_eq!((practice.phase, practice.mode), (Phase::InMatch, Some(Mode::Other)));
}

#[test]
fn a_restart_in_the_same_file_forgets_the_last_run() {
    let mut parser = Parser::new();
    for line in SESSION.lines() {
        parser.feed(line);
    }
    assert!(parser.ended());
    assert_eq!(parser.snapshot().account_id, None);
    // The next run appended.
    parser.feed("11/09 22:00:00 [Client] CL:  Connected to '127.0.0.1:27015'");
    parser.feed("11/09 22:00:00 [Client] Map: \"dl_hideout\"");
    assert!(!parser.ended());
    let snapshot = parser.snapshot();
    assert_eq!(snapshot.phase, Phase::Hideout);
    assert!(snapshot.party.is_empty() && snapshot.hero.is_none());
}

#[test]
fn a_live_state_is_valid_for_the_protocol_at_every_step() {
    for (line, snapshot) in transitions(SESSION).into_iter().chain(transitions(MODES)) {
        let live = live_state(&snapshot, Some(1_700_000_000_000));
        assert!(live.is_valid(), "{line}: {live:?}");
        let activity = with_live(
            yap_protocol::game_activity::GameActivity::playing("Deadlock", 1),
            Some(&live),
        );
        assert!(activity.is_valid(), "{line}: {activity:?}");
    }
}

#[test]
fn the_wire_shape_is_the_servers() {
    let snapshot = after(SESSION, "20:07:37 OnGameStateChanged");
    let live = live_state(&snapshot, Some(1_762_718_857_000));
    let activity = with_live(
        yap_protocol::game_activity::GameActivity::playing("Deadlock", 5),
        Some(&live),
    );
    let json = serde_json::to_value(&activity).unwrap();
    assert_eq!(
        json["game"],
        serde_json::json!({
            "kind": "deadlock", "phase": "in_match", "hero": "haze",
            "phase_started_at": 1_762_718_857_000_i64, "account_id": ME, "party": [FRIEND],
        })
    );
    assert_eq!(
        json["large_image"],
        "/api/game-assets/1276737795012165766/hero_haze_sm.png"
    );
}

#[test]
fn log_times_become_epoch_ms_from_one_anchor() {
    let anchor = at("11/09 20:10:00");
    assert_eq!(log_epoch_ms(at("11/09 20:07:37"), anchor, 1_000_000), Some(1_000_000 - 143_000));
    assert_eq!(log_epoch_ms(anchor, anchor, 5), Some(5));
    // Across New Year.
    assert_eq!(
        log_epoch_ms(at("12/31 23:59:00"), at("01/01 00:01:00"), 10_000_000),
        Some(10_000_000 - 120_000)
    );
    // Days old: unknown.
    assert_eq!(log_epoch_ms(at("11/01 20:00:00"), anchor, 10_000_000_000), None);
}

#[test]
fn deadlock_is_known_by_catalog_or_steam_id() {
    let detected = crate::Detected {
        key: "app:1276737795012165766".into(),
        name: "Deadlock".into(),
        app_id: Some(CATALOG_ID.into()),
        icon_hash: None,
        steam_app_id: None,
        started_at: 1,
        source: crate::Source::Catalog,
    };
    assert!(is_deadlock(&detected));
    let by_steam = crate::Detected {
        app_id: None,
        steam_app_id: Some("1422450".into()),
        ..detected.clone()
    };
    assert!(is_deadlock(&by_steam));
    let other = crate::Detected {
        app_id: Some("1".into()),
        steam_app_id: Some("570".into()),
        ..detected
    };
    assert!(!is_deadlock(&other));
    assert!(hero_image("haze").is_some() && hero_image("../x").is_none());
}

// ── The follower ────────────────────────────────────────────────────────────

fn write_lines(path: &Path, lines: &[&str]) {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    for line in lines {
        writeln!(file, "{line}").unwrap();
    }
}

fn now_ms() -> i64 {
    system_ms(SystemTime::now())
}

struct Run {
    follower: Follower,
    t0: Instant,
    elapsed: Duration,
    sent: Vec<Option<yap_protocol::game_activity::DeadlockLiveState>>,
    statuses: Vec<LogStatus>,
}

impl Run {
    fn new(path: &Path, game_started_at: i64) -> Self {
        let t0 = Instant::now();
        let config = WatchConfig {
            path: path.to_owned(),
            game_started_at,
            trace: false,
        };
        Self {
            follower: Follower::new(config, t0),
            t0,
            elapsed: Duration::ZERO,
            sent: Vec::new(),
            statuses: Vec::new(),
        }
    }

    /// Ticks once a second for `secs` seconds.
    fn run(&mut self, secs: u64) {
        for _ in 0..secs {
            self.elapsed += Duration::from_secs(1);
            let tick = self.follower.tick(self.t0 + self.elapsed, now_ms());
            if let Some(status) = tick.status {
                self.statuses.push(status);
            }
            if let Some(live) = tick.live {
                self.sent.push(live);
            }
        }
    }

    fn last_phase(&self) -> Option<&str> {
        self.sent.last()?.as_ref().map(|live| live.phase.as_str())
    }
}

#[test]
fn the_follower_sends_changes_once_settled() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("console.log");
    let mut run = Run::new(&path, now_ms());
    run.run(2);
    assert_eq!(run.statuses, [LogStatus::Waiting]);
    assert!(run.sent.is_empty());

    write_lines(&path, &[
        "11/09 20:01:00 [Client] CL:  Connected to '127.0.0.1:27015'",
        "11/09 20:01:00 [Client] Map: \"dl_hideout\"",
        "11/09 20:01:01 [Server] SV:  \"P<0><[U:1:1000001]><>\" STEAM USERID validated",
    ]);
    run.run(3);
    assert_eq!(run.statuses, [LogStatus::Waiting, LogStatus::Live]);
    assert_eq!(run.sent.len(), 1);
    let live = run.sent[0].clone().unwrap();
    assert_eq!((live.phase.as_str(), live.account_id), ("hideout", Some(ME)));
    let started = live.phase_started_at.unwrap();
    assert!(started <= now_ms() && started > now_ms() - 60_000, "{started}");

    // Nothing new: nothing sent, and the phase start does not drift.
    run.run(5);
    assert_eq!(run.sent.len(), 1);

    // A match forms and runs within a second: one update for both.
    write_lines(&path, &[
        "11/09 20:06:49 Lobby 1 for Match 2 created",
        "11/09 20:06:50 OnGameStateChanged: GameInProgress (7)",
    ]);
    run.run(4);
    assert_eq!(run.sent.len(), 2);
    assert_eq!(run.last_phase(), Some("in_match"));
}

#[test]
fn a_restarted_game_recreates_the_log_and_the_state_follows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("console.log");
    write_lines(&path, &[
        "11/09 20:01:00 [Client] Map: \"dl_hideout\"",
        "11/09 20:06:49 Lobby 1 for Match 2 created",
        "11/09 20:06:50 OnGameStateChanged: GameInProgress (7)",
    ]);
    let mut run = Run::new(&path, now_ms() - 1_000);
    run.run(3);
    assert_eq!(run.last_phase(), Some("in_match"));

    // The game restarted: a new, shorter file.
    std::fs::write(&path, "11/09 21:00:00 [Client] Map: \"dl_hideout\"\n").unwrap();
    run.run(5);
    assert_eq!(run.last_phase(), Some("hideout"));

    // Truncated to nothing and written again from the menu.
    std::fs::write(&path, "").unwrap();
    run.run(1);
    write_lines(&path, &["11/09 21:30:00 [RenderSystem] starting"]);
    run.run(5);
    assert_eq!(run.last_phase(), Some("menu"));
}

#[test]
fn an_old_log_from_a_run_without_condebug_is_stale() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("console.log");
    std::fs::write(&path, SESSION).unwrap();
    let old = SystemTime::now() - Duration::from_secs(3 * 86_400);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let mut run = Run::new(&path, now_ms());
    run.run(GRACE.as_secs() + 2);
    assert_eq!(run.statuses, [LogStatus::Waiting, LogStatus::Stale]);
    assert!(run.sent.iter().all(Option::is_none));
}

#[test]
fn a_missing_log_says_so_after_the_grace() {
    let dir = tempfile::tempdir().unwrap();
    let mut run = Run::new(&dir.path().join("console.log"), now_ms());
    run.run(GRACE.as_secs() + 2);
    assert_eq!(run.statuses, [LogStatus::Waiting, LogStatus::Missing]);
    assert!(run.sent.is_empty());
}

#[test]
fn a_flood_of_changes_stays_under_the_gateways_limit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("console.log");
    let mut run = Run::new(&path, now_ms());
    // A different hero every second for two minutes.
    let heroes = ["haze", "bebop", "viper", "warden"];
    write_lines(&path, &["11/09 20:01:00 [Client] Map: \"dl_hideout\""]);
    for second in 0..120 {
        let hero = heroes[second % heroes.len()];
        write_lines(&path, &[&format!(
            " VMDL Camera Pose Success! \"models/heroes_staging/{hero}/{hero}.vmdl\" has loaded camera poses"
        )]);
        run.run(1);
    }
    assert!(run.sent.len() <= 2 * throttle::PER_MINUTE, "{}", run.sent.len());
    assert!(!run.sent.is_empty());
}
