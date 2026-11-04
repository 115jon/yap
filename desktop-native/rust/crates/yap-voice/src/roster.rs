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
    pub muted: bool,
    pub speaking: bool,
    /// This client's own participant.
    pub me: bool,
}

#[derive(Debug)]
struct Row {
    name: String,
    muted: bool,
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
                muted: false,
                speaking: false,
                me: true,
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
                muted: row.muted,
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
                    muted: p.self_mute.unwrap_or(false),
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
        let row = self.rows.entry(p.id.clone()).or_insert(Row {
            name: name.clone(),
            muted: false,
            speaking: false,
            me: false,
        });
        row.name = name;
        if let Some(muted) = p.self_mute {
            row.muted = muted;
        }
    }

    pub fn remove(&mut self, id: &str) {
        self.rows.remove(id);
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
}
