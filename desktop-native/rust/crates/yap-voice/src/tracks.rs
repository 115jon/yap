//! The remote tracks of one call: what each participant offers now, what
//! is pulled (or in flight) and its receive mid, and the participants inside
//! their reconnect grace (STATE_MACHINE.md, "Reconnect grace").
//!
//! One pull per track alias (`cam-audio-<pid>`, `cam-video-<pid>`,
//! `screen-video-<pid>`, ...). Which offers get pulled is the machine's
//! call ([`TrackTable::take_pulls`]); this table only keeps the books.
//!
//! Video pulls are never closed to be pulled again: the SFU reuses a closed
//! m-line under the same mid with other RTP header extension ids, which
//! libwebrtc refuses (SCREEN_SHARE.md, "Watching"). A withdrawn video
//! publication (camera off, share stopped) is **parked**: hidden, its
//! transceiver kept, and shown again when the same SFU track is offered.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::wire::Track;

/// What a track carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Audio,
    Video,
}

impl Kind {
    fn parse(kind: &str) -> Option<Self> {
        match kind {
            "audio" => Some(Self::Audio),
            "video" => Some(Self::Video),
            _ => None,
        }
    }
}

/// A remote track to pull.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    pub participant_id: String,
    pub track_name: String,
    pub sfu_track_name: String,
    pub session_id: String,
    pub kind: Kind,
    /// The simulcast layer to ask for (`simulcast.preferredRid`); `None`
    /// for audio and single-layer video.
    pub preferred_rid: Option<String>,
    /// Pull it on a receive-only session of its own: this alias's pull was
    /// closed before, and on the same peer connection the SFU recycles the
    /// closed m-line, which libwebrtc refuses (`SplitVideoTransport`).
    pub fresh: bool,
}

/// A publication as last offered (VoiceReady or op 12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub participant_id: String,
    pub track_name: String,
    pub sfu_track_name: String,
    pub session_id: String,
    pub kind: Kind,
    /// The camera's layers, highest first (`simulcast_rids`); empty for a
    /// single layer.
    pub simulcast_rids: Vec<String>,
}

impl Offer {
    fn same_sfu_track(&self, pull: &Pull) -> bool {
        self.sfu_track_name == pull.sfu_track_name && self.session_id == pull.session_id
    }
}

/// A pulled (or in-flight) track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pull {
    pub participant_id: String,
    pub sfu_track_name: String,
    pub session_id: String,
    pub kind: Kind,
    /// The receive transceiver, once the pull worked.
    pub mid: Option<String>,
    /// Withdrawn by its publisher; kept for the same SFU track's return.
    pub parked: bool,
}

/// A participant's reconnect grace: the timer's token and the aliases they
/// offered again during it.
#[derive(Debug)]
struct Grace {
    token: u64,
    reoffered: HashSet<String>,
}

/// Remote tracks by alias. Every method that drops a pull returns the mids
/// whose receive transceivers must be closed (VOICE_PROTOCOL.md §3.3d).
#[derive(Debug, Default)]
pub struct TrackTable {
    /// Alias -> its current publication. Ordered so pulls are deterministic.
    offers: BTreeMap<String, Offer>,
    /// Alias -> its pull.
    pulls: BTreeMap<String, Pull>,
    grace: HashMap<String, Grace>,
}

impl TrackTable {
    /// Records offered tracks (not ours, only ones with an SFU name). A
    /// track without `participant_id` belongs to `outer`. A pull of the
    /// same SFU track is shown again; one of another SFU track (a
    /// republish) is retired so the new one is pulled.
    pub fn offer(&mut self, tracks: &[Track], outer: &str, me: &str) -> Vec<String> {
        let mut closing = Vec::new();
        for t in tracks {
            let pid = match t.participant_id.as_str() {
                "" => outer,
                pid => pid,
            };
            let Some(kind) = Kind::parse(&t.kind) else {
                continue;
            };
            if pid.is_empty() || pid == me || t.sfu_track_name.is_empty() {
                continue;
            }
            if let Some(grace) = self.grace.get_mut(pid) {
                grace.reoffered.insert(t.track_name.clone());
            }
            let offer = Offer {
                participant_id: pid.to_owned(),
                track_name: t.track_name.clone(),
                sfu_track_name: t.sfu_track_name.clone(),
                session_id: t.session_id.clone(),
                kind,
                simulcast_rids: t.simulcast_rids.clone(),
            };
            match self.pulls.get_mut(&t.track_name) {
                Some(pull) if offer.same_sfu_track(pull) => pull.parked = false,
                Some(_) => closing.extend(self.retire(&t.track_name)),
                None => {}
            }
            self.offers.insert(t.track_name.clone(), offer);
        }
        closing
    }

    /// Starts pulls for offers that have none and that `want` asks for
    /// (`Some(preferred_rid)`). Returns them to run.
    pub fn take_pulls(
        &mut self,
        mut want: impl FnMut(&Offer) -> Option<Option<String>>,
    ) -> Vec<Remote> {
        let mut pulls = Vec::new();
        for (alias, offer) in &self.offers {
            if self.pulls.contains_key(alias) {
                continue;
            }
            let Some(preferred_rid) = want(offer) else {
                continue;
            };
            self.pulls.insert(
                alias.clone(),
                Pull {
                    participant_id: offer.participant_id.clone(),
                    sfu_track_name: offer.sfu_track_name.clone(),
                    session_id: offer.session_id.clone(),
                    kind: offer.kind,
                    mid: None,
                    parked: false,
                },
            );
            pulls.push(Remote {
                participant_id: offer.participant_id.clone(),
                track_name: alias.clone(),
                sfu_track_name: offer.sfu_track_name.clone(),
                session_id: offer.session_id.clone(),
                kind: offer.kind,
                preferred_rid,
                fresh: false,
            });
        }
        pulls
    }

    /// Records a finished pull. A pull that worked for a track retired
    /// while it was in flight returns its mid to close; one that failed is
    /// forgotten, so a later plan pulls it again.
    pub fn pull_done(&mut self, sfu_track_name: &str, mid: Option<String>) -> Vec<String> {
        let alias = self
            .pulls
            .iter()
            .find(|(_, p)| p.sfu_track_name == sfu_track_name && p.mid.is_none())
            .map(|(alias, _)| alias.clone());
        match (alias, mid) {
            (Some(alias), Some(mid)) => {
                if let Some(pull) = self.pulls.get_mut(&alias) {
                    pull.mid = Some(mid);
                    // Withdrawn while in flight: kept, but hidden.
                    pull.parked = !self.offers.contains_key(&alias);
                }
                Vec::new()
            }
            (None, Some(mid)) => vec![mid],
            (Some(alias), None) => {
                self.pulls.remove(&alias);
                Vec::new()
            }
            (None, None) => Vec::new(),
        }
    }

    /// Forgets every pull (a new peer connection: their mids are gone with
    /// the old one); the offers stay, so they are pulled again.
    pub fn reset_pulls(&mut self) {
        self.pulls.clear();
    }

    /// Releases a pull nobody wants any more (its offer stays, so wanting it
    /// again pulls it again); its mid to close.
    pub fn release(&mut self, alias: &str) -> Option<String> {
        self.retire(alias)
    }

    /// The alias of the in-flight pull of `sfu_track_name`.
    pub fn pending_alias(&self, sfu_track_name: &str) -> Option<&str> {
        self.pulls
            .iter()
            .find(|(_, p)| p.sfu_track_name == sfu_track_name && p.mid.is_none())
            .map(|(alias, _)| alias.as_str())
    }

    /// Forgets the pull of `alias`; its mid, if it had one, to close.
    fn retire(&mut self, alias: &str) -> Option<String> {
        self.pulls.remove(alias).and_then(|p| p.mid)
    }

    /// Op 13 from `pid`: the named tracks (alias or SFU name; none means
    /// all of theirs) were withdrawn. Audio pulls close; a video pull that
    /// has its transceiver is parked.
    pub fn withdraw(&mut self, pid: &str, names: &[String]) -> Vec<String> {
        let named = |alias: &str, sfu: &str, owner: &str| {
            if names.is_empty() {
                owner == pid
            } else {
                names.iter().any(|n| n == alias || n == sfu)
            }
        };
        let mut aliases: Vec<String> = self
            .offers
            .iter()
            .filter(|(alias, o)| named(alias, &o.sfu_track_name, &o.participant_id))
            .map(|(alias, _)| alias.clone())
            .collect();
        aliases.extend(
            self.pulls
                .iter()
                .filter(|(alias, p)| named(alias, &p.sfu_track_name, &p.participant_id))
                .map(|(alias, _)| alias.clone()),
        );
        let mut closing = Vec::new();
        for alias in aliases {
            self.offers.remove(&alias);
            match self.pulls.get_mut(&alias) {
                Some(pull) if pull.kind == Kind::Video && pull.mid.is_some() => pull.parked = true,
                Some(_) => closing.extend(self.retire(&alias)),
                None => {}
            }
        }
        closing
    }

    /// Everything of `pid` but the aliases in `keep`: offers and pulls
    /// (parked ones included) go.
    pub fn drop_participant(&mut self, pid: &str, keep: &HashSet<String>) -> Vec<String> {
        let gone = |alias: &String, owner: &str| owner == pid && !keep.contains(alias);
        self.offers
            .retain(|alias, o| !gone(alias, &o.participant_id));
        let aliases: Vec<String> = self
            .pulls
            .iter()
            .filter(|(alias, p)| gone(alias, &p.participant_id))
            .map(|(alias, _)| alias.clone())
            .collect();
        aliases.iter().filter_map(|a| self.retire(a)).collect()
    }

    /// After a re-identify, VoiceReady lists every live track: offers that
    /// are not in it go, with their pulls, except parked pulls of
    /// participants still publishing something.
    pub fn keep_live(&mut self, live: &[Track]) -> Vec<String> {
        let aliases: HashSet<&str> = live.iter().map(|t| t.track_name.as_str()).collect();
        let present: HashSet<&str> = live.iter().map(|t| t.participant_id.as_str()).collect();
        self.offers
            .retain(|alias, _| aliases.contains(alias.as_str()));
        let gone: Vec<String> = self
            .pulls
            .iter()
            .filter(|(alias, p)| {
                !aliases.contains(alias.as_str())
                    && !(p.parked && present.contains(p.participant_id.as_str()))
            })
            .map(|(alias, _)| alias.clone())
            .collect();
        gone.iter().filter_map(|a| self.retire(a)).collect()
    }

    /// Starts (or restarts, forgetting what was offered during the last
    /// one) `pid`'s grace under `token`.
    pub fn start_grace(&mut self, pid: &str, token: u64) {
        self.grace.insert(
            pid.to_owned(),
            Grace {
                token,
                reoffered: HashSet::new(),
            },
        );
    }

    /// The room says `pid` left: no grace any more.
    pub fn cancel_grace(&mut self, pid: &str) {
        self.grace.remove(pid);
    }

    /// A grace timer fired. If it is `pid`'s current one, the grace ends and
    /// the aliases they offered again during it come back.
    pub fn expire_grace(&mut self, pid: &str, token: u64) -> Option<HashSet<String>> {
        if self.grace.get(pid).is_some_and(|g| g.token == token) {
            self.grace.remove(pid).map(|g| g.reoffered)
        } else {
            None
        }
    }

    /// `pid` is inside a grace.
    pub fn in_grace(&self, pid: &str) -> bool {
        self.grace.contains_key(pid)
    }

    /// The current publication of `alias`.
    pub fn offer_of(&self, alias: &str) -> Option<&Offer> {
        self.offers.get(alias)
    }

    /// The pull of `alias`.
    pub fn pull_of(&self, alias: &str) -> Option<&Pull> {
        self.pulls.get(alias)
    }

    /// Every pull, by alias.
    pub fn pulls(&self) -> impl Iterator<Item = (&str, &Pull)> {
        self.pulls.iter().map(|(alias, p)| (alias.as_str(), p))
    }

    /// Aliases pulled or in flight.
    pub fn pulled_names(&self) -> Vec<&str> {
        self.pulls.keys().map(String::as_str).collect()
    }

    /// Offers not pulled (or in flight) yet.
    pub fn waiting(&self) -> Vec<&Offer> {
        self.offers
            .iter()
            .filter(|(alias, _)| !self.pulls.contains_key(*alias))
            .map(|(_, o)| o)
            .collect()
    }

    /// `alias` is pulled, has its transceiver and is not parked: its media
    /// arrives.
    pub fn is_live(&self, alias: &str) -> bool {
        self.pulls
            .get(alias)
            .is_some_and(|p| p.mid.is_some() && !p.parked)
    }
}
