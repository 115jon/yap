//! The remote tracks of one call: what is announced but not pulled yet,
//! what is pulled (or in flight) and its receive mid, and the participants
//! inside their re-announce grace.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::wire::Track;

/// A remote audio track to pull.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    pub participant_id: String,
    pub track_name: String,
    pub sfu_track_name: String,
    pub session_id: String,
}

/// A pulled (or in-flight) track.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pulled {
    owner: String,
    /// The publisher's track alias (`track_name`).
    alias: String,
    /// The receive transceiver, once the pull worked.
    mid: Option<String>,
}

/// Remote tracks by SFU track name. Every method that drops a pulled track
/// returns the mids whose receive transceivers must be closed (§3.3d).
#[derive(Debug, Default)]
pub struct TrackTable {
    /// Announced, waiting for media to be up (§3.3c).
    pending: Vec<Remote>,
    /// SFU track name -> pull. Ordered so closes are deterministic.
    pulled: BTreeMap<String, Pulled>,
    /// Participant -> the token of their grace timer.
    grace: HashMap<String, u64>,
}

impl TrackTable {
    /// Queues the remote audio tracks in `tracks` (not ours, and only ones
    /// with an SFU name). A track without `participant_id` belongs to
    /// `outer`.
    pub fn queue(&mut self, tracks: &[Track], outer: &str, me: &str) {
        for t in tracks {
            let pid = match t.participant_id.as_str() {
                "" => outer,
                pid => pid,
            };
            if pid == me || t.kind != "audio" || t.sfu_track_name.is_empty() {
                continue;
            }
            self.pending.push(Remote {
                participant_id: pid.to_owned(),
                track_name: t.track_name.clone(),
                sfu_track_name: t.sfu_track_name.clone(),
                session_id: t.session_id.clone(),
            });
        }
    }

    /// Moves what is waiting into the pulled set and returns the pulls to
    /// start. A track already pulled or in flight is skipped.
    pub fn take_pulls(&mut self) -> Vec<Remote> {
        let mut pulls = Vec::new();
        for remote in std::mem::take(&mut self.pending) {
            if self.pulled.contains_key(&remote.sfu_track_name) {
                continue;
            }
            self.pulled.insert(
                remote.sfu_track_name.clone(),
                Pulled {
                    owner: remote.participant_id.clone(),
                    alias: remote.track_name.clone(),
                    mid: None,
                },
            );
            pulls.push(remote);
        }
        pulls
    }

    /// Records a finished pull. A pull that worked for a track retired
    /// while it was in flight returns its mid to close.
    pub fn pull_done(&mut self, sfu_track_name: &str, mid: Option<String>) -> Vec<String> {
        match (self.pulled.get_mut(sfu_track_name), mid) {
            (Some(pulled), Some(mid)) => {
                pulled.mid = Some(mid);
                Vec::new()
            }
            (None, Some(mid)) => vec![mid],
            (_, None) => {
                self.pulled.remove(sfu_track_name);
                Vec::new()
            }
        }
    }

    /// Forgets these pulls; returns their mids to close.
    pub fn retire(&mut self, gone: &[String]) -> Vec<String> {
        gone.iter()
            .filter_map(|sfu| self.pulled.remove(sfu))
            .filter_map(|pulled| pulled.mid)
            .collect()
    }

    /// Everything of `pid`: pulls retired (mids returned), waiting dropped.
    pub fn drop_participant(&mut self, pid: &str) -> Vec<String> {
        let gone = self.owned_by(pid);
        self.pending.retain(|r| r.participant_id != pid);
        self.retire(&gone)
    }

    /// Retires `pid`'s pulls (what is waiting stays); returns mids to close.
    pub fn retire_pulls_of(&mut self, pid: &str) -> Vec<String> {
        let gone = self.owned_by(pid);
        self.retire(&gone)
    }

    /// Drops what is waiting from `pid` (their pulls stay).
    pub fn drop_pending_of(&mut self, pid: &str) {
        self.pending.retain(|r| r.participant_id != pid);
    }

    /// After a re-identify, VoiceReady lists every live track: retire pulls
    /// and drop waiting tracks that are not in it.
    pub fn keep_live(&mut self, live: &HashSet<&str>) -> Vec<String> {
        let gone: Vec<String> = self
            .pulled
            .keys()
            .filter(|k| !live.contains(k.as_str()))
            .cloned()
            .collect();
        self.pending
            .retain(|r| live.contains(r.sfu_track_name.as_str()));
        self.retire(&gone)
    }

    /// A publication announced under a new SFU name replaces the pull of
    /// its old one; the same name keeps its pull.
    pub fn replace_republished(&mut self, tracks: &[Track], outer: &str) -> Vec<String> {
        let mut replaced = Vec::new();
        for t in tracks {
            let pid = match t.participant_id.as_str() {
                "" => outer,
                pid => pid,
            };
            replaced.extend(
                self.pulled
                    .iter()
                    .filter(|(sfu, p)| {
                        p.owner == pid && p.alias == t.track_name && **sfu != t.sfu_track_name
                    })
                    .map(|(sfu, _)| sfu.clone()),
            );
        }
        self.retire(&replaced)
    }

    /// Op 13 from `pid`: only the named tracks stopped (a screen share can
    /// stop while the mic stays); no names means all of theirs. Their
    /// transceivers are closed so a republish is pulled again.
    pub fn stop(&mut self, pid: &str, names: &[String]) -> Vec<String> {
        let stopped = |sfu: &str, owner: &str, alias: &str| {
            if names.is_empty() {
                owner == pid
            } else {
                names.iter().any(|n| n == sfu || n == alias)
            }
        };
        let gone: Vec<String> = self
            .pulled
            .iter()
            .filter(|(sfu, p)| stopped(sfu, &p.owner, &p.alias))
            .map(|(sfu, _)| sfu.clone())
            .collect();
        self.pending
            .retain(|r| !stopped(&r.sfu_track_name, &r.participant_id, &r.track_name));
        self.retire(&gone)
    }

    /// The SFU track names pulled from `pid`.
    fn owned_by(&self, pid: &str) -> Vec<String> {
        self.pulled
            .iter()
            .filter(|(_, p)| p.owner == pid)
            .map(|(sfu, _)| sfu.clone())
            .collect()
    }

    /// Starts (or restarts) `pid`'s grace under `token`.
    pub fn start_grace(&mut self, pid: &str, token: u64) {
        self.grace.insert(pid.to_owned(), token);
    }

    /// `pid` re-announced in time.
    pub fn cancel_grace(&mut self, pid: &str) {
        self.grace.remove(pid);
    }

    /// A grace timer fired: `true` (and the grace ends) only if it is the
    /// current one for `pid`, not one cancelled or restarted since.
    pub fn expire_grace(&mut self, pid: &str, token: u64) -> bool {
        if self.grace.get(pid) == Some(&token) {
            self.grace.remove(pid);
            true
        } else {
            false
        }
    }

    /// SFU track names pulled or in flight.
    pub fn pulled_names(&self) -> Vec<&str> {
        self.pulled.keys().map(String::as_str).collect()
    }

    /// Tracks waiting for media.
    pub fn pending(&self) -> &[Remote] {
        &self.pending
    }
}
