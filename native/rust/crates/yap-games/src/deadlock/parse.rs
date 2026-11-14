//! Deadlock's `console.log`, one line at a time, into a small state machine:
//! the phase, the hero, the mode, when the phase began, the account playing
//! and the party.
//!
//! The signals are the ones Deadlock RPC (HeyTariq/deadlock-rpc, GPL-3.0)
//! reads, reimplemented here from the line shapes the game writes (checked
//! against a 2026 log; see the fixtures). Each shape is one entry of
//! [`signal`], so a patch that rewords a line is a one-line fix.
//!
//! Lines look like `11/09 20:31:05 [Client] Map: "dl_hideout"`: the local
//! month, day and time, then the text. Some (the camera poses) have no time.

/// Where the player is, as the protocol names it (`DEADLOCK_LIVE_PHASES`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Menu,
    Hideout,
    Queue,
    MatchIntro,
    InMatch,
    PostMatch,
    Spectating,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Menu => "menu",
            Phase::Hideout => "hideout",
            Phase::Queue => "queue",
            Phase::MatchIntro => "match_intro",
            Phase::InMatch => "in_match",
            Phase::PostMatch => "post_match",
            Phase::Spectating => "spectating",
        }
    }

    fn in_match(self) -> bool {
        matches!(self, Phase::MatchIntro | Phase::InMatch)
    }
}

/// The match's mode (`DEADLOCK_LIVE_MODES`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Ranked,
    Unranked,
    Brawl,
    Bots,
    Private,
    Other,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Ranked => "ranked",
            Mode::Unranked => "unranked",
            Mode::Brawl => "brawl",
            Mode::Bots => "bots",
            Mode::Private => "private",
            Mode::Other => "other",
        }
    }
}

/// How sure a mode is: a later signal replaces an earlier one only when it
/// is at least as sure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Sureness {
    /// A local server or a practice map.
    Guess,
    /// The player count.
    Count,
    /// Bots set up for the match.
    Bots,
    /// The game named it (`k_ECitadelMatchMode_*`).
    Named,
}

/// A line's own clock: seconds since January 1 at midnight, local time,
/// with no year (the log has none). Only differences are used.
pub type LogSecs = u32;

/// Days before each month (February as 29 days: a leap day is a valid
/// date; a non-leap March is one day late, which only matters across it).
const DAYS_BEFORE: [u32; 12] = [0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335];
/// A year of [`LogSecs`] (for a phase that began before New Year).
pub const YEAR_SECS: u32 = 366 * 86_400;

/// `MM/DD HH:MM:SS ` at the start of a line: its time and the rest.
pub fn line_time(line: &str) -> Option<(LogSecs, &str)> {
    let bytes = line.as_bytes();
    if bytes.len() < 15 || bytes[2] != b'/' || bytes[5] != b' ' || bytes[8] != b':' || bytes[11] != b':' {
        return None;
    }
    let num = |at: usize| -> Option<u32> {
        let pair = line.get(at..at + 2)?;
        pair.bytes().all(|b| b.is_ascii_digit()).then(|| pair.parse().ok())?
    };
    let (month, day, hour, minute, second) = (num(0)?, num(3)?, num(6)?, num(9)?, num(12)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let days = DAYS_BEFORE[(month - 1) as usize] + day - 1;
    let secs = days * 86_400 + hour * 3_600 + minute * 60 + second.min(59);
    Some((secs, line[14..].trim_start()))
}

/// What one line says. Lines not listed say nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Signal<'a> {
    /// `[Client] Map: "dl_hideout"`, `[Client|Server] Created physics for dl_midtown`.
    Map(&'a str),
    /// `[Client] CL:  Connected to 'loopback:1'` (or an address).
    Connected(&'a str),
    /// `[Client] Disconnecting from server: NETWORK_DISCONNECT_EXITING`.
    Disconnected(&'a str),
    /// `Entering matchmaking for region 0 ...`, the GC's enter/start messages.
    QueueStart,
    /// The GC's stop message (a party event also stops it).
    QueueStop,
    /// `Lobby 1 for Match 2 created`, `CCitadel_PlayController::OnMatchFormed`.
    MatchFormed,
    /// `Lobby 1 for Match 2 destroyed`.
    LobbyDestroyed,
    /// `Playing Broadcast` (watching a match).
    Spectating,
    /// `ChangeGameState: GameInProgress (7)` (the local server) or
    /// `OnGameStateChanged: PostGame (8)` (the client).
    GameState(&'a str, Option<u32>),
    /// `[Hideout] Hideout Lobby Connection State: NoLobby (0)`.
    HideoutLobby(Option<i32>),
    /// `VMDL Camera Pose Success! "models/heroes_staging/haze/haze.vmdl" ...`:
    /// the camera for the hero being played.
    HeroPose(&'a str),
    /// `[Client] Players: 12 (1 bots) / 32 humans`.
    Players { total: u32, bots: u32 },
    /// `Initializing bot for player slot 2: k_ECitadelBotDifficulty_Easy`.
    BotInit,
    /// `k_ECitadelMatchMode_Ranked` anywhere in a line.
    NamedMode(&'a str),
    /// `LoopMode: menu`.
    MenuLoop,
    /// `Host activate: Quitting`, `Dispatching EventAppShutdown_t`, `Source2Shutdown`.
    Shutdown,
}

/// The first signal a line's text carries (the time already taken off).
fn signal(text: &str) -> Option<Signal<'_>> {
    if let Some(rest) = text.strip_prefix("[Client] Map:") {
        let (name, _) = rest.trim().strip_prefix('"')?.split_once('"')?;
        return Some(Signal::Map(name));
    }
    for prefix in ["[Client] Created physics for", "[Server] Created physics for"] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return Some(Signal::Map(rest.split_whitespace().next()?));
        }
    }
    if let Some(rest) = text.strip_prefix("[Client] CL:") {
        let rest = rest.trim_start().strip_prefix("Connected to '")?;
        return Some(Signal::Connected(rest.split('\'').next()?));
    }
    if let Some(rest) = text.strip_prefix("[Client] Disconnecting from server:") {
        return Some(Signal::Disconnected(rest.trim()));
    }
    if text.starts_with("[HostStateManager] Host activate: Quitting")
        || text.starts_with("Dispatching EventAppShutdown_t")
        || text == "Source2Shutdown"
    {
        return Some(Signal::Shutdown);
    }
    if text.starts_with("Entering matchmaking")
        || text.contains("k_EMsgServerToGCEnterMatchmaking")
        || text.contains("k_EMsgClientToGCStartMatchmaking")
    {
        return Some(Signal::QueueStart);
    }
    if text.contains("k_EMsgClientToGCStopMatchmaking") {
        return Some(Signal::QueueStop);
    }
    if let Some(rest) = text.strip_prefix("Lobby ") {
        if rest.contains(" for Match ") {
            if rest.ends_with(" created") {
                return Some(Signal::MatchFormed);
            }
            if rest.ends_with(" destroyed") {
                return Some(Signal::LobbyDestroyed);
            }
        }
    }
    if text.contains("CCitadel_PlayController::OnMatchFormed") {
        return Some(Signal::MatchFormed);
    }
    if text.starts_with("Playing Broadcast") {
        return Some(Signal::Spectating);
    }
    for prefix in ["ChangeGameState:", "OnGameStateChanged:"] {
        if let Some(rest) = text.strip_prefix(prefix) {
            let (name, id) = name_and_number(rest);
            return Some(Signal::GameState(name, id.and_then(|n| u32::try_from(n).ok())));
        }
    }
    if let Some(rest) = text.strip_prefix("[Hideout] Hideout Lobby Connection State:") {
        let (_, id) = name_and_number(rest);
        return Some(Signal::HideoutLobby(id.and_then(|n| i32::try_from(n).ok())));
    }
    if text.starts_with("VMDL Camera Pose Success!") {
        return hero_from_model(text).map(Signal::HeroPose);
    }
    if let Some(rest) = text.strip_prefix("[Client] Players:") {
        // `12 (1 bots) / 32 humans`
        let mut words = rest.split_whitespace();
        let total = words.next()?.parse().ok()?;
        let bots = words.next()?.strip_prefix('(')?.parse().ok()?;
        return Some(Signal::Players { total, bots });
    }
    if text.starts_with("Initializing bot for player slot") {
        return Some(Signal::BotInit);
    }
    if let Some(at) = text.find("k_ECitadelMatchMode_") {
        let rest = &text[at + "k_ECitadelMatchMode_".len()..];
        let end = rest
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .unwrap_or(rest.len());
        return Some(Signal::NamedMode(&rest[..end]));
    }
    if text.contains("LoopMode: menu") {
        return Some(Signal::MenuLoop);
    }
    None
}

/// ` Name (7)` → (`Name`, Some(7)).
fn name_and_number(text: &str) -> (&str, Option<i64>) {
    let text = text.trim();
    let name = text.split_whitespace().next().unwrap_or("");
    let number = text
        .rsplit_once('(')
        .and_then(|(_, rest)| rest.strip_suffix(')'))
        .and_then(|n| n.trim().parse().ok());
    (name, number)
}

/// The hero folder of `"models/heroes[_wip|_staging|...]/<hero>/..."`.
fn hero_from_model(text: &str) -> Option<&str> {
    let at = text.find("models/heroes")?;
    let rest = &text[at + "models/heroes".len()..];
    let rest = &rest[rest.find('/')? + 1..];
    let name = rest.split('/').next()?;
    (!name.is_empty() && rest.len() > name.len()).then_some(name)
}

/// A hero's class name without `hero_`, lower case, with a model version
/// (`_v2`) taken off; `None` when it would not pass the protocol.
pub fn hero_key(raw: &str) -> Option<String> {
    let lower = raw.trim().to_ascii_lowercase();
    let lower = lower.strip_prefix("hero_").unwrap_or(&lower);
    let key = match lower.rsplit_once('_') {
        Some((stem, version))
            if version.len() > 1
                && version.starts_with('v')
                && version[1..].bytes().all(|b| b.is_ascii_digit()) =>
        {
            stem
        }
        _ => lower,
    };
    let ok = (1..=40).contains(&key.len())
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    ok.then(|| key.to_owned())
}

/// `CMsgGCToClientPartyEvent: { party_id: 1 event: k_eJoinedParty initiator_account_id: 2 }`.
fn party_event(text: &str) -> Option<(u64, &str, u32)> {
    let rest = text.strip_prefix("CMsgGCToClientPartyEvent:")?;
    let field = |name: &str| -> Option<&str> {
        let at = rest.find(name)? + name.len();
        rest[at..].split_whitespace().next()
    };
    let party = field("party_id:")?.parse().ok()?;
    let event = field("event:")?;
    let account = field("initiator_account_id:")?.trim_end_matches('}').parse().ok()?;
    Some((party, event, account))
}

/// `[U:1:<id>]` in a line.
fn steam3_account(text: &str) -> Option<u32> {
    let at = text.find("[U:1:")? + "[U:1:".len();
    let digits = text[at..].split(']').next()?;
    digits.parse().ok().filter(|&id| id > 0)
}

/// Most others a party keeps (Deadlock parties are 6).
const PARTY_KEEP: usize = 5;

/// What the log says now.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Snapshot {
    pub phase: Phase,
    pub hero: Option<String>,
    pub mode: Option<Mode>,
    /// When the phase began, on the log's clock.
    pub phase_since: Option<LogSecs>,
    pub account_id: Option<u32>,
    /// The others in the party (not the player), oldest first, at most 5.
    pub party: Vec<u32>,
}

/// The state machine. Feed it every line in order; a shutdown line starts
/// it over.
#[derive(Debug, Clone, Default)]
pub struct Parser {
    phase: Phase,
    since: Option<LogSecs>,
    /// The newest line time seen.
    now: Option<LogSecs>,
    hero: Option<String>,
    /// The match's hero is the first camera after the match formed (the
    /// rest are kill cams and spectated players).
    hero_locked: bool,
    mode: Option<(Mode, Sureness)>,
    /// A mode named before the match formed (in the queue).
    queued_mode: Option<Mode>,
    most_players: u32,
    most_bots: u32,
    /// Connected to a server on this computer (the Hideout, a sandbox).
    local_server: bool,
    account_id: Option<u32>,
    party_id: Option<u64>,
    party: Vec<u32>,
    /// The last thing the log said was the game quitting.
    ended: bool,
}

impl Parser {
    pub fn new() -> Self {
        Self::default()
    }

    /// The newest line time seen.
    pub fn now(&self) -> Option<LogSecs> {
        self.now
    }

    /// Whether the log's last word was the game quitting (an earlier run's
    /// log, if the game is running now).
    pub fn ended(&self) -> bool {
        self.ended
    }

    pub fn snapshot(&self) -> Snapshot {
        let shows_hero = !matches!(self.phase, Phase::Menu | Phase::Spectating);
        let shows_mode = matches!(self.phase, Phase::MatchIntro | Phase::InMatch | Phase::PostMatch);
        Snapshot {
            phase: self.phase,
            hero: self.hero.clone().filter(|_| shows_hero),
            mode: self.mode.map(|(mode, _)| mode).filter(|_| shows_mode),
            phase_since: self.since,
            account_id: self.account_id,
            party: {
                // The newest five (a missed leave must not hide a join).
                let others: Vec<u32> = self
                    .party
                    .iter()
                    .copied()
                    .filter(|&id| Some(id) != self.account_id)
                    .collect();
                others[others.len().saturating_sub(PARTY_KEEP)..].to_vec()
            },
        }
    }

    /// One line, as the file has it (a BOM, `\r` and spaces are fine).
    pub fn feed(&mut self, line: &str) {
        let line = line.trim_start_matches('\u{feff}').trim_end();
        let text = match line_time(line) {
            Some((time, text)) => {
                self.now = Some(time);
                // The menu began with the log.
                self.since.get_or_insert(time);
                text
            }
            None => line.trim_start(),
        };
        if text.is_empty() {
            return;
        }
        // The account: the local server's own check of the player (the
        // first one; other players are never validated on this computer).
        if self.account_id.is_none() && text.contains("STEAM USERID validated") {
            self.account_id = steam3_account(text);
        }
        if let Some((party, event, account)) = party_event(text) {
            self.party_event(party, event, account);
            return;
        }
        if let Some(signal) = signal(text) {
            let quits = matches!(signal, Signal::Shutdown)
                || matches!(signal, Signal::Disconnected(reason) if reason.contains("EXITING"));
            self.apply(signal);
            if !quits {
                self.ended = false;
            }
        }
    }

    fn enter(&mut self, phase: Phase) {
        if phase == self.phase {
            return;
        }
        self.phase = phase;
        self.since = self.now;
        match phase {
            Phase::Menu | Phase::Spectating => {
                self.hero = None;
                self.mode = None;
            }
            Phase::Hideout => {
                // The Hideout loads the hero held there again.
                self.hero = None;
                self.hero_locked = false;
                self.mode = None;
            }
            Phase::MatchIntro => {
                self.hero = None;
                self.hero_locked = false;
                self.mode = self.queued_mode.take().map(|mode| (mode, Sureness::Named));
                self.most_players = 0;
                self.most_bots = 0;
            }
            Phase::Queue | Phase::InMatch | Phase::PostMatch => {}
        }
    }

    fn set_mode(&mut self, mode: Mode, sureness: Sureness) {
        if self.mode.is_none_or(|(_, had)| sureness >= had) {
            self.mode = Some((mode, sureness));
        }
    }

    /// The mode from the player count, once the match runs (players are
    /// still joining in the intro).
    fn count_mode(&mut self) {
        if self.phase != Phase::InMatch || self.local_server {
            return;
        }
        let (total, bots) = (self.most_players, self.most_bots);
        if total >= 4 && bots * 2 > total {
            self.set_mode(Mode::Bots, Sureness::Count);
        } else if (4..=8).contains(&total) {
            self.set_mode(Mode::Brawl, Sureness::Count);
        }
    }

    fn apply(&mut self, signal: Signal<'_>) {
        match signal {
            Signal::Map(name) => {
                let map = name.to_ascii_lowercase();
                match map.as_str() {
                    "" | "<empty>" | "start" => {}
                    // Queued players wait in the Hideout, and a party
                    // reconnects them to the leader's.
                    "dl_hideout" if self.phase == Phase::Queue => {}
                    "dl_hideout" => self.enter(Phase::Hideout),
                    _ => {
                        if matches!(
                            self.phase,
                            Phase::Menu | Phase::Hideout | Phase::Queue | Phase::PostMatch
                        ) {
                            self.enter(Phase::MatchIntro);
                        }
                        if map == "new_player_basics" {
                            self.set_mode(Mode::Other, Sureness::Guess);
                        } else if self.local_server && self.phase.in_match() {
                            // A match map on this computer: a sandbox.
                            self.set_mode(Mode::Other, Sureness::Guess);
                        }
                    }
                }
            }
            Signal::Connected(address) => {
                let address = address.to_ascii_lowercase();
                self.local_server =
                    address.starts_with("loopback") || address.starts_with("127.0.0.1");
            }
            Signal::Disconnected(reason) => {
                if reason.contains("EXITING") {
                    self.start_over();
                } else if reason.contains("LOOPDEACTIVATE") {
                    // The engine switching loops, not leaving.
                } else if self.phase.in_match() {
                    self.enter(Phase::PostMatch);
                } else if self.phase == Phase::Spectating {
                    self.enter(Phase::Menu);
                }
            }
            Signal::QueueStart => {
                if matches!(self.phase, Phase::Menu | Phase::Hideout | Phase::PostMatch) {
                    self.enter(Phase::Queue);
                }
            }
            Signal::QueueStop => self.leave_queue(),
            Signal::MatchFormed => {
                if !self.phase.in_match() {
                    self.enter(Phase::MatchIntro);
                }
            }
            Signal::LobbyDestroyed => {
                if self.phase.in_match() {
                    self.enter(Phase::PostMatch);
                }
            }
            Signal::Spectating => self.enter(Phase::Spectating),
            Signal::GameState(name, id) => {
                // The Hideout runs the same states on its own server; only
                // a match's count.
                if !self.phase.in_match() {
                    return;
                }
                let name = name.to_ascii_lowercase();
                let running = match name.as_str() {
                    "gameinprogress" | "inprogress" => true,
                    "postgame" | "end" => false,
                    "waitingforplayerstojoin" | "heroselection" | "matchintro"
                    | "waitformaptoload" | "pregamewait" => return,
                    _ => match id {
                        Some(7) => true,
                        Some(8 | 11) => false,
                        _ => return,
                    },
                };
                if running {
                    if self.phase == Phase::MatchIntro {
                        self.enter(Phase::InMatch);
                        self.count_mode();
                    }
                } else {
                    self.enter(Phase::PostMatch);
                }
            }
            Signal::HideoutLobby(id) => {
                // No lobby: playing alone.
                if id == Some(0) && matches!(self.phase, Phase::Menu | Phase::Hideout | Phase::Queue) {
                    self.clear_party();
                }
            }
            Signal::HeroPose(raw) => {
                let Some(hero) = hero_key(raw) else { return };
                match self.phase {
                    Phase::Hideout | Phase::Queue => self.hero = Some(hero),
                    Phase::MatchIntro | Phase::InMatch if !self.hero_locked => {
                        self.hero = Some(hero);
                        self.hero_locked = true;
                    }
                    _ => {}
                }
            }
            Signal::Players { total, bots } => {
                if self.phase.in_match() {
                    self.most_players = self.most_players.max(total);
                    self.most_bots = self.most_bots.max(bots);
                    self.count_mode();
                }
            }
            Signal::BotInit => {
                if self.phase.in_match() && !self.local_server {
                    self.set_mode(Mode::Bots, Sureness::Bots);
                }
            }
            Signal::NamedMode(name) => {
                let mode = match name.to_ascii_lowercase().as_str() {
                    "ranked" | "calibration" => Mode::Ranked,
                    "unranked" => Mode::Unranked,
                    "privatelobby" | "private" => Mode::Private,
                    "coopbot" | "botmatch" | "bots" => Mode::Bots,
                    "streetbrawl" | "brawl" => Mode::Brawl,
                    "invalid" => return,
                    _ => Mode::Other,
                };
                match self.phase {
                    Phase::MatchIntro | Phase::InMatch | Phase::PostMatch => {
                        self.set_mode(mode, Sureness::Named);
                    }
                    _ => self.queued_mode = Some(mode),
                }
            }
            Signal::MenuLoop => {
                if self.phase.in_match() {
                    self.enter(Phase::PostMatch);
                } else if self.phase == Phase::Spectating {
                    self.enter(Phase::Menu);
                }
            }
            Signal::Shutdown => self.start_over(),
        }
    }

    fn leave_queue(&mut self) {
        if self.phase == Phase::Queue {
            self.queued_mode = None;
            self.enter(Phase::Hideout);
        }
    }

    fn party_event(&mut self, party: u64, event: &str, account: u32) {
        let event = event.to_ascii_lowercase();
        let me = self.account_id == Some(account);
        if event.contains("joinedparty") {
            if me || self.party_id != Some(party) {
                // A party we joined, or a new one: its earlier members are
                // never announced.
                self.party_id = Some(party);
                self.party.clear();
            }
            if !me && !self.party.contains(&account) && account > 0 {
                self.party.push(account);
                if self.party.len() > PARTY_KEEP + 1 {
                    self.party.remove(0);
                }
            }
        } else if event.contains("leftparty")
            || event.contains("removedfromparty")
            || event.contains("kickedfromparty")
        {
            if me {
                self.clear_party();
            } else {
                self.party.retain(|&id| id != account);
            }
        } else if event.contains("disband") {
            self.clear_party();
        } else if event.contains("matchmakingstopped") {
            self.leave_queue();
        } else if event.contains("matchmakingstarted") {
            self.apply(Signal::QueueStart);
        }
    }

    fn clear_party(&mut self) {
        self.party_id = None;
        self.party.clear();
    }

    /// The game quit: nothing carries over to its next run.
    fn start_over(&mut self) {
        let now = self.now;
        *self = Self::new();
        self.now = now;
        self.since = now;
        self.ended = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(parser: &mut Parser, text: &str) {
        for line in text.lines() {
            parser.feed(line);
        }
    }

    #[test]
    fn line_times_are_read_and_odd_lines_have_none() {
        let (secs, rest) = line_time("01/02 03:04:05 [Client] Map: \"x\"").unwrap();
        assert_eq!(secs, 86_400 + 3 * 3_600 + 4 * 60 + 5);
        assert_eq!(rest, "[Client] Map: \"x\"");
        assert!(line_time(" VMDL Camera Pose Success!").is_none());
        assert!(line_time("13/02 03:04:05 x").is_none());
        assert!(line_time("1/02 03:04:05 x").is_none());
    }

    #[test]
    fn hero_keys_follow_the_protocol() {
        assert_eq!(hero_key("hero_haze").as_deref(), Some("haze"));
        assert_eq!(hero_key("Tengu_v2").as_deref(), Some("tengu"));
        assert_eq!(hero_key("vampirebat").as_deref(), Some("vampirebat"));
        assert_eq!(hero_key("gun_slinger_v").as_deref(), Some("gun_slinger_v"));
        assert!(hero_key("bad-name").is_none());
        assert!(hero_key(&"x".repeat(41)).is_none());
    }

    #[test]
    fn the_hero_comes_from_the_camera_model_path() {
        let line = r#" VMDL Camera Pose Success! "models/heroes_staging/haze/haze.vmdl" has loaded camera poses"#;
        assert_eq!(signal(line.trim()), Some(Signal::HeroPose("haze")));
        let wip = r#"VMDL Camera Pose Success! "models/heroes_wip/tengu/tengu_v2/tengu.vmdl""#;
        assert_eq!(signal(wip), Some(Signal::HeroPose("tengu")));
        let plain = r#"VMDL Camera Pose Success! "models/heroes/wraith/wraith.vmdl""#;
        assert_eq!(signal(plain), Some(Signal::HeroPose("wraith")));
    }

    #[test]
    fn hideout_game_states_do_not_start_a_match() {
        let mut p = Parser::new();
        feed(
            &mut p,
            "11/09 20:00:01 [Client] Map: \"dl_hideout\"\n\
             11/09 20:00:02 ChangeGameState: MatchIntro (4)\n\
             11/09 20:00:02 ChangeGameState: GameInProgress (7)\n\
             11/09 20:00:03 OnGameStateChanged: GameInProgress (7)",
        );
        assert_eq!(p.snapshot().phase, Phase::Hideout);
    }

    #[test]
    fn a_shutdown_starts_over() {
        let mut p = Parser::new();
        feed(
            &mut p,
            "11/09 20:00:01 [Server] SV:  \"P<0><[U:1:1000001]><>\" STEAM USERID validated\n\
             11/09 20:00:01 [Client] Map: \"dl_hideout\"\n\
             11/09 20:10:00 [HostStateManager] Host activate: Quitting",
        );
        let snap = p.snapshot();
        assert_eq!(snap.phase, Phase::Menu);
        assert_eq!(snap.account_id, None);
        assert_eq!(snap.phase_since, Some(line_time("11/09 20:10:00 x").unwrap().0));
    }

    #[test]
    fn a_named_mode_in_the_queue_carries_into_the_match() {
        let mut p = Parser::new();
        feed(
            &mut p,
            "11/09 20:00:01 [Client] Map: \"dl_hideout\"\n\
             11/09 20:00:05 Entering matchmaking for region 0 with search key ''\n\
             11/09 20:00:05 matchmaking mode k_ECitadelMatchMode_Ranked\n\
             11/09 20:01:00 Lobby 5 for Match 6 created",
        );
        assert_eq!(p.snapshot().mode, Some(Mode::Ranked));
        feed(&mut p, "11/09 20:01:01 [Client] Players: 6 (0 bots) / 32 humans");
        feed(&mut p, "11/09 20:01:30 OnGameStateChanged: GameInProgress (7)");
        // A count never replaces a mode the game named.
        assert_eq!(p.snapshot().mode, Some(Mode::Ranked));
    }

    #[test]
    fn game_states_by_number_when_the_name_is_new() {
        let mut p = Parser::new();
        feed(
            &mut p,
            "11/09 20:01:00 Lobby 5 for Match 6 created\n\
             11/09 20:01:30 OnGameStateChanged: Renamed (7)",
        );
        assert_eq!(p.snapshot().phase, Phase::InMatch);
        feed(&mut p, "11/09 20:30:00 OnGameStateChanged: AlsoRenamed (8)");
        assert_eq!(p.snapshot().phase, Phase::PostMatch);
    }

    #[test]
    fn overlong_and_garbage_lines_are_harmless() {
        let mut p = Parser::new();
        p.feed(&"\u{fffd}".repeat(5000));
        p.feed("[Client] Map: \"unterminated");
        p.feed("CMsgGCToClientPartyEvent: { party_id: x event: y }");
        p.feed("[Client] Players: lots");
        assert_eq!(p.snapshot(), Snapshot::default());
    }
}
