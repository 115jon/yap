//! Who is in the call, as the room reports it.

use std::collections::{BTreeMap, HashMap};

use crate::wire::Participant;

/// The name a participant shows before any full state named them.
const FALLBACK_NAME: &str = "Someone";

/// One row of the roster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// The participant id.
    pub id: String,
    pub name: String,
    /// The Yap user id, once a full state named it (empty before).
    pub user_id: String,
    pub muted: bool,
    pub deafened: bool,
    pub speaking: bool,
    /// This client's own participant.
    pub me: bool,
}

#[derive(Debug, Default)]
struct Row {
    name: String,
    user_id: String,
    muted: bool,
    deafened: bool,
    speaking: bool,
    me: bool,
}

/// The roster, ordered by participant id. Names come from full voice
/// states only; deltas lack them, so names are kept for rows a delta
/// re-adds.
#[derive(Debug)]
pub struct Roster {
    me: String,
    rows: BTreeMap<String, Row>,
    known_names: HashMap<String, String>,
}

impl Roster {
    /// A roster with only us.
    pub fn new(me: &str, my_name: &str) -> Self {
        let mut rows = BTreeMap::new();
        rows.insert(
            me.to_owned(),
            Row {
                name: my_name.to_owned(),
                me: true,
                ..Row::default()
            },
        );
        Self {
            me: me.to_owned(),
            rows,
            known_names: HashMap::new(),
        }
    }

    /// The whole roster, in a stable order.
    pub fn snapshot(&self) -> Vec<Member> {
        self.rows
            .iter()
            .map(|(id, row)| Member {
                id: id.clone(),
                name: row.name.clone(),
                user_id: row.user_id.clone(),
                muted: row.muted,
                deafened: row.deafened,
                speaking: row.speaking,
                me: row.me,
            })
            .collect()
    }

    /// Replaces the others with a room Ready/Resumed's `participants` (a
    /// full state; it excludes us). Speaking bits carry over.
    pub fn replace_others(&mut self, participants: &[Participant]) {
        let speaking: HashMap<String, bool> = self
            .rows
            .iter()
            .map(|(id, row)| (id.clone(), row.speaking))
            .collect();
        let me = self.me.as_str();
        self.rows.retain(|id, _| id == me);
        for p in participants {
            if p.id.is_empty() || p.id == self.me {
                continue;
            }
            let name = p.name.clone().unwrap_or_else(|| FALLBACK_NAME.to_owned());
            self.known_names.insert(p.id.clone(), name.clone());
            self.rows.insert(
                p.id.clone(),
                Row {
                    name,
                    user_id: p.user_id.clone().unwrap_or_default(),
                    muted: p.self_mute.unwrap_or(false),
                    deafened: p.self_deaf.unwrap_or(false),
                    speaking: speaking.get(&p.id).copied().unwrap_or(false),
                    me: false,
                },
            );
        }
    }

    /// An op 15 join or update for someone else (full state or delta).
    pub fn upsert(&mut self, p: &Participant) {
        if let Some(name) = &p.name {
            self.known_names.insert(p.id.clone(), name.clone());
        }
        let name = self
            .known_names
            .get(&p.id)
            .cloned()
            .unwrap_or_else(|| FALLBACK_NAME.to_owned());
        let row = self.rows.entry(p.id.clone()).or_default();
        row.name = name;
        if let Some(user_id) = &p.user_id {
            row.user_id.clone_from(user_id);
        }
        if let Some(muted) = p.self_mute {
            row.muted = muted;
        }
        if let Some(deafened) = p.self_deaf {
            row.deafened = deafened;
        }
    }

    pub fn remove(&mut self, id: &str) {
        self.rows.remove(id);
    }

    /// A participant's Yap user id, once a full state named it.
    pub fn user_id_of(&self, id: &str) -> Option<&str> {
        self.rows
            .get(id)
            .map(|row| row.user_id.as_str())
            .filter(|user_id| !user_id.is_empty())
    }

    /// Sets a row's speaking bit; `false` if there is no such row.
    pub fn set_speaking(&mut self, id: &str, speaking: bool) -> bool {
        match self.rows.get_mut(id) {
            Some(row) => {
                row.speaking = speaking;
                true
            }
            None => false,
        }
    }

    pub fn set_my_mute(&mut self, muted: bool) {
        if let Some(row) = self.rows.get_mut(&self.me) {
            row.muted = muted;
        }
    }

    pub fn set_my_deaf(&mut self, deafened: bool) {
        if let Some(row) = self.rows.get_mut(&self.me) {
            row.deafened = deafened;
        }
    }
}
