//! Spatial audio, as the web places people (`src/lib/voice/spatial-audio.ts`):
//! the room's shared state (op 2 Ready, op 15 and op 9 carry it as
//! `spatial_audio_state`; we send ours in op 15), the seat each person gets
//! from it, and the pan and distance gain of each peer as heard from our
//! seat. Pure; the machine applies the result to the voice tracks.

use std::collections::{BTreeMap, HashMap};

use serde_json::{json, Map, Value};

/// How the room seats people (`SpatialPlacementMode`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Placement {
    Line,
    #[default]
    Arc,
    Grid,
    Manual,
}

impl Placement {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "line" => Self::Line,
            "arc" => Self::Arc,
            "grid" => Self::Grid,
            "manual" => Self::Manual,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Arc => "arc",
            Self::Grid => "grid",
            Self::Manual => "manual",
        }
    }
}

/// A seat on the room map, 0..=100 on both axes (x left to right).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

/// Where we sit when the map has no seat for us (`useVoiceChannel.ts`).
pub const DEFAULT_SELF_POSITION: Position = Position { x: 50.0, y: 78.0 };

/// The room's shared placement (`SharedSpatialAudioState`).
#[derive(Debug, Clone, PartialEq)]
pub struct SpatialState {
    pub enabled: bool,
    pub placement: Placement,
    /// Square metres, 10..=100.
    pub room_size: f64,
    /// 10..=95.
    pub distance: f64,
    /// Degrees, 30..=180.
    pub arc_angle: f64,
    /// Seats by Yap user id, for [`Placement::Manual`].
    pub manual: BTreeMap<String, Position>,
    pub updated_by: Option<String>,
    /// Epoch milliseconds.
    pub updated_at: f64,
}

impl Default for SpatialState {
    /// `DEFAULT_SHARED_SPATIAL_STATE`.
    fn default() -> Self {
        Self {
            enabled: false,
            placement: Placement::Arc,
            room_size: 40.0,
            distance: 55.0,
            arc_angle: 120.0,
            manual: BTreeMap::new(),
            updated_by: None,
            updated_at: 0.0,
        }
    }
}

fn number(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64).filter(|n| n.is_finite())
}

impl SpatialState {
    /// `normalizeSpatialState`: the defaults under what is given, and the
    /// three sizes rounded and clamped. `None` for anything but an object.
    pub fn from_value(v: &Value) -> Option<Self> {
        let object = v.as_object()?;
        let d = Self::default();
        let manual = object
            .get("manualPositions")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .filter_map(|(id, p)| {
                        Some((
                            id.clone(),
                            Position {
                                x: number(p, "x")?,
                                y: number(p, "y")?,
                            },
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(Self {
            enabled: v.get("enabled").and_then(Value::as_bool).unwrap_or(false),
            placement: v
                .get("placementMode")
                .and_then(Value::as_str)
                .and_then(Placement::parse)
                .unwrap_or(d.placement),
            room_size: number(v, "roomSize")
                .unwrap_or(d.room_size)
                .round()
                .clamp(10.0, 100.0),
            distance: number(v, "distance")
                .unwrap_or(d.distance)
                .round()
                .clamp(10.0, 95.0),
            arc_angle: number(v, "arcAngle")
                .unwrap_or(d.arc_angle)
                .round()
                .clamp(30.0, 180.0),
            manual,
            updated_by: v
                .get("updatedBy")
                .and_then(Value::as_str)
                .map(str::to_owned),
            updated_at: number(v, "updatedAt").unwrap_or(0.0),
        })
    }

    /// As op 15 sends it (and the server's `isSpatialAudioState` takes it).
    pub fn to_value(&self) -> Value {
        let manual: Map<String, Value> = self
            .manual
            .iter()
            .map(|(id, p)| (id.clone(), json!({ "x": p.x, "y": p.y })))
            .collect();
        let mut v = json!({
            "enabled": self.enabled,
            "placementMode": self.placement.as_str(),
            "roomSize": self.room_size,
            "distance": self.distance,
            "arcAngle": self.arc_angle,
            "manualPositions": manual,
            "updatedAt": self.updated_at,
        });
        if let Some(by) = &self.updated_by {
            v["updatedBy"] = json!(by);
        }
        v
    }
}

/// Our side of spatial audio: the switches that decide whether we hear it,
/// and who we are on the map.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpatialLocal {
    /// "Spatial audio" (`spatialAudioEnabled`).
    pub enabled: bool,
    /// "High fidelity audio" (`streamHighFidelity`): spatial audio needs it.
    pub high_fidelity: bool,
    /// Our Yap user id (the map seats people by user id).
    pub user_id: String,
}

impl SpatialLocal {
    /// Whether we hear the room spatially (`applySpatialAudio`'s
    /// `enabledForLocal`): the room has it on, so do we, with high
    /// fidelity, and we are not deafened.
    pub fn hears(&self, room: &SpatialState, deafened: bool) -> bool {
        room.enabled && self.enabled && self.high_fidelity && !deafened
    }
}

fn clamp(v: f64, min: f64, max: f64) -> f64 {
    v.clamp(min, max)
}

fn auto_position(index: usize, count: usize, mode: Placement, distance: f64, arc: f64) -> Position {
    let (i, n) = (index as f64, count as f64);
    match mode {
        Placement::Line => {
            let span = (distance * 1.25).clamp(24.0, 76.0);
            let x = if count == 1 {
                50.0
            } else {
                50.0 - span / 2.0 + span * i / (n - 1.0)
            };
            Position {
                x: clamp(x, 8.0, 92.0),
                y: clamp(48.0 - distance * 0.18, 14.0, 66.0),
            }
        }
        Placement::Grid => {
            let cols = n.sqrt().ceil();
            let rows = (n / cols).ceil();
            let col = i % cols;
            let row = (i / cols).floor();
            let width = (distance * 1.35).clamp(34.0, 78.0);
            let height = (distance * 0.8).clamp(24.0, 48.0);
            let x = if cols == 1.0 {
                50.0
            } else {
                50.0 - width / 2.0 + width * col / (cols - 1.0)
            };
            let y = if rows == 1.0 {
                44.0
            } else {
                30.0 + height * row / (rows - 1.0)
            };
            Position {
                x: clamp(x, 8.0, 92.0),
                y: clamp(y, 12.0, 70.0),
            }
        }
        // Manual seats fall back to the arc for anyone without one.
        Placement::Arc | Placement::Manual => {
            let spread = if count == 1 { 0.0 } else { arc };
            let start = -spread / 2.0;
            let step = if count == 1 {
                0.0
            } else {
                spread * i / (n - 1.0)
            };
            let angle = (start + step).to_radians();
            let (rx, ry) = (distance * 0.52, distance * 0.34);
            Position {
                x: clamp(50.0 + angle.sin() * rx, 8.0, 92.0),
                y: clamp(70.0 - angle.cos() * ry, 12.0, 72.0),
            }
        }
    }
}

/// Everyone's seat (`calculateSpatialPositions`), by user id, for people
/// in this order (we come first, as the web lists us).
pub fn positions(people: &[&str], state: &SpatialState) -> HashMap<String, Position> {
    let count = people.len();
    people
        .iter()
        .enumerate()
        .map(|(index, id)| {
            let seat = match state.placement {
                Placement::Manual => state.manual.get(*id).copied(),
                _ => None,
            };
            let seat = seat.unwrap_or_else(|| {
                auto_position(
                    index,
                    count,
                    state.placement,
                    state.distance,
                    state.arc_angle,
                )
            });
            ((*id).to_owned(), seat)
        })
        .collect()
}

/// How one peer sounds from our seat (`calculateSpatialAudioMix`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mix {
    /// -1 (left) ..= 1 (right).
    pub pan: f64,
    /// 0.32..=1, falling off with distance.
    pub gain: f64,
    pub distance_meters: f64,
}

/// No spatial audio: centred and at full gain.
pub const FLAT: Mix = Mix {
    pan: 0.0,
    gain: 1.0,
    distance_meters: 0.0,
};

pub fn mix(me: Option<Position>, peer: Option<Position>, room_size: f64) -> Mix {
    let (Some(me), Some(peer)) = (me, peer) else {
        return FLAT;
    };
    let side = clamp(room_size, 10.0, 100.0).sqrt();
    let dx = (peer.x - me.x) / 100.0 * side;
    let dy = (peer.y - me.y) / 100.0 * side;
    let distance = (dx * dx + dy * dy).sqrt();
    let pan = clamp(dx / (side * 0.38).max(1.2), -1.0, 1.0);
    let start = (side * 0.12).max(0.75);
    let range = (side * 0.6).max(1.5);
    let rolloff = clamp((distance - start) / range, 0.0, 1.0);
    Mix {
        pan,
        gain: clamp(1.0 - rolloff * 0.68, 0.32, 1.0),
        distance_meters: distance,
    }
}

/// Each peer's mix as we hear the room: `me` first, then `peers` in order
/// (user ids). Everyone is [`FLAT`] while we do not hear it spatially.
pub fn mixes(
    me: &str,
    peers: &[&str],
    state: &SpatialState,
    spatial: bool,
) -> HashMap<String, Mix> {
    if !spatial {
        return peers.iter().map(|p| ((*p).to_owned(), FLAT)).collect();
    }
    let mut people = Vec::with_capacity(peers.len() + 1);
    people.push(me);
    people.extend(peers.iter().copied().filter(|p| *p != me));
    let seats = positions(&people, state);
    let mine = seats.get(me).copied().or(Some(DEFAULT_SELF_POSITION));
    peers
        .iter()
        .map(|p| {
            (
                (*p).to_owned(),
                mix(mine, seats.get(*p).copied(), state.room_size),
            )
        })
        .collect()
}

/// One person on the room map as the spatial panel draws them: their seat
/// (as [`positions`] gives it, so the map shows what the mix plays) and
/// whether they hear spatial audio (the panel's "not hearing" note).
#[derive(Debug, Clone, PartialEq)]
pub struct MapSeat {
    /// Yap user id (ours may be the participant id before we know it).
    pub user_id: String,
    pub me: bool,
    pub position: Position,
    /// Their "Spatial audio" switch (`spatial_audio_enabled`).
    pub enabled: bool,
    /// Their "High fidelity audio" switch.
    pub high_fidelity: bool,
}

/// What the spatial panel (SpatialAudioPanel.tsx) shows: the room's state
/// and everyone's seat, us first.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpatialMap {
    pub state: SpatialState,
    pub seats: Vec<MapSeat>,
}

impl SpatialMap {
    /// Seats `me` and then `peers` (user id, enabled, high fidelity), once
    /// each, as [`mixes`] seats them.
    pub fn new(state: &SpatialState, me: (&str, bool, bool), peers: &[(&str, bool, bool)]) -> Self {
        let mut people = Vec::with_capacity(peers.len() + 1);
        people.push(me);
        people.extend(peers.iter().copied().filter(|p| p.0 != me.0));
        let ids: Vec<&str> = people.iter().map(|p| p.0).collect();
        let seats = positions(&ids, state);
        Self {
            state: state.clone(),
            seats: people
                .iter()
                .enumerate()
                .map(|(index, &(id, enabled, high_fidelity))| MapSeat {
                    user_id: id.to_owned(),
                    me: index == 0,
                    position: seats
                        .get(id)
                        .copied()
                        .unwrap_or(Position { x: 50.0, y: 50.0 }),
                    enabled,
                    high_fidelity,
                })
                .collect(),
        }
    }

    /// `{state, seats: [{userId, me, x, y, enabled, highFidelity}]}`.
    pub fn to_value(&self) -> Value {
        let seats: Vec<Value> = self
            .seats
            .iter()
            .map(|s| {
                json!({
                    "userId": s.user_id,
                    "me": s.me,
                    "x": s.position.x,
                    "y": s.position.y,
                    "enabled": s.enabled,
                    "highFidelity": s.high_fidelity,
                })
            })
            .collect();
        json!({ "state": self.state.to_value(), "seats": seats })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn normalizing_should_round_and_clamp_the_sizes() {
        let s = SpatialState::from_value(&json!({
            "enabled": true, "roomSize": 400, "distance": 3.6, "arcAngle": 90.4,
            "placementMode": "grid", "manualPositions": {}, "updatedAt": 5,
        }))
        .expect("state");
        assert_eq!(
            (s.enabled, s.placement, s.room_size, s.distance, s.arc_angle),
            (true, Placement::Grid, 100.0, 10.0, 90.0)
        );
    }

    #[test]
    fn missing_fields_should_take_the_web_defaults() {
        let s = SpatialState::from_value(&json!({})).expect("state");
        assert_eq!(
            s,
            SpatialState {
                updated_at: 0.0,
                ..SpatialState::default()
            }
        );
    }

    #[test]
    fn the_state_should_round_trip_through_json() {
        let mut s = SpatialState {
            enabled: true,
            placement: Placement::Manual,
            updated_by: Some("u1".into()),
            updated_at: 1234.0,
            ..SpatialState::default()
        };
        s.manual.insert("u2".into(), Position { x: 20.0, y: 30.0 });
        assert_eq!(SpatialState::from_value(&s.to_value()), Some(s));
    }

    #[test]
    fn one_person_on_an_arc_should_sit_in_the_middle() {
        let seats = positions(&["me"], &SpatialState::default());
        let me = seats["me"];
        assert!(close(me.x, 50.0) && close(me.y, 70.0 - 55.0 * 0.34));
    }

    #[test]
    fn an_arc_of_three_should_match_the_web() {
        // calculateSpatialPositions with the defaults (arc 120, distance 55).
        let seats = positions(&["a", "b", "c"], &SpatialState::default());
        let a = seats["a"];
        let expected_x = 50.0 + (-60f64).to_radians().sin() * 55.0 * 0.52;
        let expected_y = 70.0 - (-60f64).to_radians().cos() * 55.0 * 0.34;
        assert!(close(a.x, expected_x) && close(a.y, expected_y), "{a:?}");
        assert!(close(seats["b"].x, 50.0));
    }

    #[test]
    fn manual_seats_should_win_and_others_fall_back_to_the_arc() {
        let mut state = SpatialState {
            placement: Placement::Manual,
            ..SpatialState::default()
        };
        state
            .manual
            .insert("b".into(), Position { x: 90.0, y: 20.0 });
        let seats = positions(&["a", "b"], &state);
        assert_eq!(seats["b"], Position { x: 90.0, y: 20.0 });
        assert!(seats["a"].x < 50.0);
    }

    #[test]
    fn a_peer_to_the_right_should_pan_right() {
        let m = mix(
            Some(Position { x: 50.0, y: 50.0 }),
            Some(Position { x: 80.0, y: 50.0 }),
            40.0,
        );
        // side √40 ≈ 6.32 m, dx 1.90 m over max(1.2, 2.40) m.
        assert!((m.pan - 0.79).abs() < 0.01, "{m:?}");
    }

    #[test]
    fn a_far_peer_should_be_quieter_but_not_below_the_floor() {
        let far = mix(
            Some(Position { x: 0.0, y: 0.0 }),
            Some(Position { x: 100.0, y: 100.0 }),
            100.0,
        );
        assert!(close(far.gain, 0.32) && close(far.pan, 1.0), "{far:?}");
    }

    #[test]
    fn no_seat_should_be_flat() {
        assert_eq!(mix(None, Some(Position { x: 1.0, y: 1.0 }), 40.0), FLAT);
    }

    #[test]
    fn hearing_it_should_need_the_room_us_and_high_fidelity() {
        let room = SpatialState {
            enabled: true,
            ..SpatialState::default()
        };
        let mut local = SpatialLocal {
            enabled: true,
            high_fidelity: true,
            user_id: "me".into(),
        };
        assert!(local.hears(&room, false));
        assert!(!local.hears(&room, true));
        local.high_fidelity = false;
        assert!(!local.hears(&room, false));
    }

    fn mode(placement: Placement, distance: f64, arc_angle: f64) -> SpatialState {
        SpatialState {
            placement,
            distance,
            arc_angle,
            ..SpatialState::default()
        }
    }

    // spatial-audio.test.ts "places line participants evenly".
    #[test]
    fn a_line_should_space_people_evenly_at_one_depth() {
        let seats = positions(&["u1", "u2", "u3"], &mode(Placement::Line, 60.0, 120.0));
        let (u1, u2, u3) = (seats["u1"], seats["u2"], seats["u3"]);
        assert!(u1.x < u2.x && close(u2.x, 50.0) && u3.x > u2.x);
        assert!(close(u1.y, u3.y));
    }

    // "honors arc angle and distance".
    #[test]
    fn a_wider_arc_further_out_should_spread_people_further_apart() {
        let narrow = positions(&["u1", "u2"], &mode(Placement::Arc, 40.0, 60.0));
        let wide = positions(&["u1", "u2"], &mode(Placement::Arc, 80.0, 160.0));
        assert!((wide["u2"].x - wide["u1"].x).abs() > (narrow["u2"].x - narrow["u1"].x).abs());
    }

    // "keeps grid positions within bounds".
    #[test]
    fn a_full_grid_should_stay_inside_the_room() {
        let ids = ["u1", "u2", "u3", "u4", "u5", "u6", "u7", "u8", "u9"];
        let seats = positions(&ids, &mode(Placement::Grid, 95.0, 120.0));
        for p in seats.values() {
            assert!(
                (8.0..=92.0).contains(&p.x) && (12.0..=70.0).contains(&p.y),
                "{p:?}"
            );
        }
    }

    #[test]
    fn the_map_should_seat_us_first_with_the_mixes_seats() {
        let room = SpatialState {
            placement: Placement::Line,
            ..SpatialState::default()
        };
        let map = SpatialMap::new(
            &room,
            ("me", true, true),
            &[("a", true, false), ("me", true, true), ("b", false, true)],
        );
        let ids: Vec<&str> = map.seats.iter().map(|s| s.user_id.as_str()).collect();
        assert_eq!(ids, ["me", "a", "b"]);
        let seats = positions(&["me", "a", "b"], &room);
        assert!(map.seats.iter().all(|s| s.position == seats[&s.user_id]));
        assert!(map.seats[0].me && !map.seats[1].me);
        assert_eq!(
            (map.seats[1].high_fidelity, map.seats[2].enabled),
            (false, false)
        );
    }

    #[test]
    fn the_map_json_should_carry_the_state_and_each_seat() {
        let map = SpatialMap::new(&SpatialState::default(), ("me", false, true), &[]);
        let v = map.to_value();
        assert_eq!(v["state"]["placementMode"], json!("arc"));
        assert_eq!(v["seats"][0]["userId"], json!("me"));
        assert_eq!(v["seats"][0]["me"], json!(true));
        assert_eq!(v["seats"][0]["enabled"], json!(false));
        assert_eq!(v["seats"][0]["x"], json!(50.0));
    }

    #[test]
    fn mixes_should_be_flat_when_not_spatial() {
        let m = mixes("me", &["a"], &SpatialState::default(), false);
        assert_eq!(m["a"], FLAT);
    }

    #[test]
    fn with_two_peers_on_the_default_arc_they_should_pan_apart() {
        let room = SpatialState {
            enabled: true,
            ..SpatialState::default()
        };
        let m = mixes("me", &["a", "b"], &room, true);
        // We sit leftmost (first on the arc): both are to our right, b more.
        assert!(m["a"].pan > 0.0 && m["b"].pan > m["a"].pan, "{m:?}");
    }
}
