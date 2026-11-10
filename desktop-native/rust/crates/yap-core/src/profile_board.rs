//! Profile boards' shared model (`src/lib/profile-board.ts`,
//! `src/lib/profile-board-editor.ts`, `board-format.ts`): the widget specs,
//! the two-column Rack packing both clients render, the editor's reducer
//! (add, remove, move, resize, config edits, undo and redo) and the
//! readouts' number and date formats. Pure: no I/O, no clock.
//!
//! The server is authoritative (`parseBoard` cleans every save), so the
//! editor keeps text as typed and only enforces what the picker and the
//! toolbar offer: the sizes a type takes, per-type caps and 12 widgets.

use serde_json::{json, Map, Value};
use yap_protocol::profile_board::{Board, Widget};

use crate::store::time::{civil_date, local_day, parse_rfc3339, LocalOffset};

// ── Widgets ────────────────────────────────────────────────────────────────

/// `WidgetSpec`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spec {
    pub kind: &'static str,
    pub name: &'static str,
    pub blurb: &'static str,
    pub category: &'static str,
    pub sizes: &'static [&'static str],
    pub default_size: &'static str,
    pub max: usize,
    /// `customWidgets` for Custom.
    pub perk: bool,
    /// The legend's glyph (`WIDGET_ICON`).
    pub icon: &'static str,
}

const SML: &[&str] = &["S", "M", "L"];
const SM: &[&str] = &["S", "M"];
const ML: &[&str] = &["M", "L"];

/// `[kind, name, blurb, category]` and the layout facts.
const fn spec(
    [kind, name, blurb, category]: [&'static str; 4],
    sizes: &'static [&'static str],
    default_size: &'static str,
    max: usize,
    icon: &'static str,
) -> Spec {
    Spec {
        kind,
        name,
        blurb,
        category,
        sizes,
        default_size,
        max,
        perk: false,
        icon,
    }
}

/// `WIDGET_SPECS`, in `WIDGET_TYPES` order.
pub const SPECS: [Spec; 14] = [
    spec(
        [
            "now_playing",
            "Now playing",
            "What you're in right now, or the last thing you played.",
            "games",
        ],
        SML,
        "M",
        1,
        "gamepad",
    ),
    spec(
        [
            "top_games",
            "Top games",
            "Your most played over 7 days, 30 days or all time.",
            "games",
        ],
        SML,
        "M",
        1,
        "trophy",
    ),
    spec(
        [
            "deadlock",
            "Deadlock",
            "Your rank, heroes and recent matches, and what you're playing right now.",
            "games",
        ],
        SML,
        "M",
        1,
        "sword",
    ),
    spec(
        [
            "voice_hours",
            "Voice hours",
            "Hours in calls this month, day by day.",
            "voice",
        ],
        SML,
        "M",
        1,
        "mic",
    ),
    spec(
        [
            "listening",
            "Listening",
            "The track you're sharing in Listen Together, or the last one.",
            "voice",
        ],
        SM,
        "S",
        1,
        "headphones",
    ),
    spec(
        [
            "badges",
            "Badge case",
            "Pick up to 8 badges and their order.",
            "show",
        ],
        SML,
        "M",
        1,
        "verified",
    ),
    spec(
        [
            "servers",
            "Favourite servers",
            "Up to 4 of your servers.",
            "social",
        ],
        SML,
        "M",
        1,
        "server",
    ),
    spec(
        [
            "quote",
            "Pinned message",
            "Pin a message you sent.",
            "social",
        ],
        SML,
        "L",
        1,
        "pin",
    ),
    spec(
        [
            "guestbook",
            "Guestbook",
            "People you share a server or DM with leave short notes.",
            "social",
        ],
        ML,
        "M",
        1,
        "rules",
    ),
    spec(
        [
            "mood",
            "Mood",
            "Yappy in a pose, with a line about how it's going.",
            "show",
        ],
        SM,
        "S",
        1,
        "emoji",
    ),
    spec(
        [
            "photos",
            "Photo strip",
            "Up to 4 pictures with a caption.",
            "show",
        ],
        SML,
        "M",
        2,
        "image",
    ),
    spec(
        [
            "links",
            "Links",
            "Up to 5 links, each with a label.",
            "show",
        ],
        SM,
        "S",
        2,
        "link",
    ),
    spec(
        [
            "countdown",
            "Countdown",
            "Days and hours to something you're looking forward to.",
            "show",
        ],
        SM,
        "S",
        1,
        "timer",
    ),
    Spec {
        perk: true,
        ..spec(
            [
                "custom",
                "Custom",
                "Your own plate: a title, a picture and four short lines.",
                "yours",
            ],
            SML,
            "L",
            2,
            "sparkle",
        )
    },
];

/// `WIDGET_CATEGORIES` with their labels, in the picker's order.
pub const CATEGORIES: [(&str, &str); 5] = [
    ("games", "Games"),
    ("voice", "Voice & music"),
    ("show", "Show off"),
    ("social", "Social"),
    ("yours", "Yours"),
];

/// Most widgets one board holds.
pub const MAX_WIDGETS: usize = 12;
/// Columns in the Rack.
pub const RACK_COLUMNS: u32 = 2;
/// One Rack row, in pixels (the web's `RACK_ROW_UNIT`).
pub const RACK_ROW_UNIT: u32 = 168;
/// Undo steps kept.
pub const MAX_HISTORY: usize = 100;
/// Notes the guestbook widget shows (`GUESTBOOK_PREVIEW`).
pub const GUESTBOOK_PREVIEW: usize = 3;
/// `TEXT_LIMITS.guestbook`.
pub const GUESTBOOK_LIMIT: usize = 140;

/// `TEXT_LIMITS`, for the editor's fields.
pub fn text_limit(field: &str) -> usize {
    match field {
        "legend" | "linkLabel" => 24,
        "title" | "countdownTitle" | "wishlistName" => 40,
        "line" => 48,
        "mood" => 60,
        "caption" | "alt" | "wishlistHint" => 80,
        "url" => 200,
        "quote" => 280,
        "guestbook" => GUESTBOOK_LIMIT,
        _ => 80,
    }
}

pub fn spec_of(kind: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|spec| spec.kind == kind)
}

/// The spec's name, or the kind for an unknown one.
pub fn widget_name(widget: &Widget) -> String {
    if widget.kind == "custom" {
        if let Some(legend) = widget.cfg.get("legend").and_then(Value::as_str) {
            if !legend.trim().is_empty() {
                return legend.trim().to_owned();
            }
        }
    }
    spec_of(&widget.kind).map_or_else(|| widget.kind.clone(), |spec| spec.name.to_owned())
}

/// `defaultWidgetConfig`.
pub fn default_config(kind: &str) -> Value {
    match kind {
        "now_playing" => json!({ "when_idle": "last_played" }),
        "top_games" => json!({ "range": "30d" }),
        "deadlock" => json!({ "at_rest": "career", "range": "30d", "show_party": true }),
        "voice_hours" => json!({ "period": "month" }),
        "badges" => json!({ "badge_ids": [] }),
        "servers" => json!({ "server_ids": [] }),
        "quote" => json!({ "message_id": null, "snapshot": null }),
        "mood" => json!({ "pose": "bark", "text": "", "clears_at": null }),
        "photos" => json!({ "photos": [], "caption": "" }),
        "links" => json!({ "links": [] }),
        "countdown" => json!({ "title": "", "at": null }),
        "custom" => json!({ "legend": "", "title": "", "asset": null, "lines": [] }),
        _ => json!({}),
    }
}

const ALPHABET: &[u8; 36] = b"abcdefghijklmnopqrstuvwxyz0123456789";

/// A widget id: 10 characters of [a-z0-9] from `seed` (the store mixes its
/// clock and a counter in, so ids never repeat on one board).
pub fn widget_id(seed: u64) -> String {
    // SplitMix64, so neighbouring seeds give unrelated ids.
    let mut x = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut out = String::with_capacity(10);
    for _ in 0..10 {
        x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        x ^= x >> 31;
        out.push(ALPHABET[(x % 36) as usize] as char);
    }
    out
}

/// `createWidget`.
pub fn create_widget(kind: &str, size: Option<&str>, seed: u64) -> Option<Widget> {
    let spec = spec_of(kind)?;
    let size = size
        .filter(|s| spec.sizes.contains(s))
        .unwrap_or(spec.default_size);
    Some(Widget {
        id: widget_id(seed),
        kind: kind.to_owned(),
        size: size.to_owned(),
        cfg: default_config(kind),
    })
}

/// `remainingOfType`: how many more of `kind` fit on `board`.
pub fn remaining_of_type(board: &Board, kind: &str) -> usize {
    let Some(spec) = spec_of(kind) else {
        return 0;
    };
    if board.widgets.len() >= MAX_WIDGETS {
        return 0;
    }
    let used = board.widgets.iter().filter(|w| w.kind == kind).count();
    spec.max.saturating_sub(used)
}

// ── The Rack ───────────────────────────────────────────────────────────────

/// A widget's place on the Rack: 0-based column and row, and its span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RackCell {
    pub col: u32,
    pub row: u32,
    pub cols: u32,
    pub rows: u32,
}

/// `RACK_SPANS`: S is half width × 1 row, M half width × 2, L full × 1.
pub fn rack_span(size: &str) -> (u32, u32) {
    match size {
        "M" => (1, 2),
        "L" => (2, 1),
        _ => (1, 1),
    }
}

/// `packRack`: each widget takes the first free slot in board order,
/// scanning rows then columns (`grid-auto-flow: dense`). Returns the cells
/// and the rows used.
pub fn pack_rack<'a>(sizes: impl IntoIterator<Item = &'a str>) -> (Vec<RackCell>, u32) {
    let mut taken: Vec<[bool; RACK_COLUMNS as usize]> = Vec::new();
    let free = |taken: &Vec<[bool; 2]>, row: u32, col: u32| {
        taken
            .get(row as usize)
            .is_none_or(|cells| !cells[col as usize])
    };
    let mut cells = Vec::new();
    let mut rows = 0;
    for size in sizes {
        let (cols, span_rows) = rack_span(size);
        let mut row = 0;
        let placed = loop {
            let mut found = None;
            for col in 0..=(RACK_COLUMNS - cols) {
                let fits =
                    (row..row + span_rows).all(|r| (col..col + cols).all(|c| free(&taken, r, c)));
                if fits {
                    found = Some(col);
                    break;
                }
            }
            if let Some(col) = found {
                break RackCell {
                    col,
                    row,
                    cols,
                    rows: span_rows,
                };
            }
            row += 1;
        };
        for r in placed.row..placed.row + placed.rows {
            while taken.len() <= r as usize {
                taken.push([false; 2]);
            }
            for c in placed.col..placed.col + placed.cols {
                taken[r as usize][c as usize] = true;
            }
        }
        rows = rows.max(placed.row + placed.rows);
        cells.push(placed);
    }
    (cells, rows)
}

// ── The editor ─────────────────────────────────────────────────────────────

/// `PreviewAudience`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Preview {
    #[default]
    Me,
    Friend,
    Anyone,
}

impl Preview {
    pub fn parse(text: &str) -> Self {
        match text {
            "friend" => Self::Friend,
            "anyone" => Self::Anyone,
            _ => Self::Me,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Me => "me",
            Self::Friend => "friend",
            Self::Anyone => "anyone",
        }
    }
}

/// `BoardEditorState`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Editor {
    /// The board as the server last confirmed it, and its version.
    pub saved: Board,
    pub version: i64,
    pub draft: Board,
    pub past: Vec<Board>,
    pub future: Vec<Board>,
    pub selected: Option<String>,
    /// Picked up with Space, moved with the arrow keys.
    pub grabbed: Option<String>,
    pub preview: Preview,
    last_coalesce: Option<String>,
}

/// `BoardEditorAction`.
#[derive(Debug, Clone, PartialEq)]
pub enum EditAction {
    /// A new widget of a type, at the end or at `index`; `seed` makes its id.
    Add {
        kind: String,
        index: Option<usize>,
        seed: u64,
    },
    Remove(String),
    Move {
        id: String,
        index: usize,
    },
    MoveBy {
        id: String,
        delta: i64,
    },
    Resize {
        id: String,
        size: String,
    },
    CycleSize {
        id: String,
        up: bool,
    },
    /// Merges `cfg` into the widget's config. Edits with the same
    /// `coalesce` key in a row are one undo step.
    UpdateCfg {
        id: String,
        cfg: Map<String, Value>,
        coalesce: Option<String>,
    },
    Undo,
    Redo,
    Discard,
    Saved {
        board: Board,
        version: i64,
    },
    Select(Option<String>),
    Grab(Option<String>),
    Preview(Preview),
}

fn same(a: &Board, b: &Board) -> bool {
    a.widgets == b.widgets
}

impl Editor {
    /// `createEditorState`.
    pub fn new(board: Board, version: i64) -> Self {
        Self {
            saved: board.clone(),
            version,
            draft: board,
            ..Self::default()
        }
    }

    /// `isDirty`.
    pub fn dirty(&self) -> bool {
        !same(&self.draft, &self.saved)
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    fn index_of(&self, id: &str) -> Option<usize> {
        self.draft.widgets.iter().position(|w| w.id == id)
    }

    fn commit(&mut self, widgets: Vec<Widget>, coalesce: Option<String>) {
        if widgets == self.draft.widgets {
            return;
        }
        let continuing = coalesce.is_some() && coalesce == self.last_coalesce;
        let previous = std::mem::replace(&mut self.draft, Board { v: 1, widgets });
        if !continuing {
            self.past.push(previous);
            if self.past.len() > MAX_HISTORY {
                self.past.remove(0);
            }
        }
        self.future.clear();
        self.last_coalesce = coalesce;
    }

    fn keep_selection(board: &Board, id: Option<String>) -> Option<String> {
        id.filter(|id| board.widgets.iter().any(|w| &w.id == id))
    }

    /// `boardEditorReducer`.
    pub fn apply(&mut self, action: EditAction) {
        match action {
            EditAction::Add { kind, index, seed } => {
                if remaining_of_type(&self.draft, &kind) == 0 {
                    return;
                }
                let Some(mut widget) = create_widget(&kind, None, seed) else {
                    return;
                };
                // Never reuse an id already on the board.
                let mut salt = seed;
                while self.draft.widgets.iter().any(|w| w.id == widget.id) {
                    salt = salt.wrapping_add(1);
                    widget.id = widget_id(salt);
                }
                let mut next = self.draft.widgets.clone();
                let at = index.unwrap_or(next.len()).min(next.len());
                let id = widget.id.clone();
                next.insert(at, widget);
                self.commit(next, None);
                self.selected = Some(id);
                self.grabbed = None;
            }
            EditAction::Remove(id) => {
                let Some(index) = self.index_of(&id) else {
                    return;
                };
                let mut next = self.draft.widgets.clone();
                next.remove(index);
                // Selection moves to the neighbour, so Delete works again.
                let neighbour = next
                    .get(index.min(next.len().saturating_sub(1)))
                    .map(|w| w.id.clone());
                self.commit(next, None);
                if self.selected.as_deref() == Some(&id) {
                    self.selected = neighbour;
                }
                if self.grabbed.as_deref() == Some(&id) {
                    self.grabbed = None;
                }
            }
            EditAction::Move { id, index } => {
                let Some(from) = self.index_of(&id) else {
                    return;
                };
                let mut next = self.draft.widgets.clone();
                let widget = next.remove(from);
                next.insert(index.min(next.len()), widget);
                self.commit(next, None);
            }
            EditAction::MoveBy { id, delta } => {
                let Some(from) = self.index_of(&id) else {
                    return;
                };
                let to = (from as i64 + delta).max(0) as usize;
                self.apply(EditAction::Move { id, index: to });
            }
            EditAction::Resize { id, size } => {
                let Some(index) = self.index_of(&id) else {
                    return;
                };
                let ok = spec_of(&self.draft.widgets[index].kind)
                    .is_some_and(|spec| spec.sizes.contains(&size.as_str()));
                if !ok {
                    return;
                }
                let mut next = self.draft.widgets.clone();
                next[index].size = size;
                self.commit(next, None);
            }
            EditAction::CycleSize { id, up } => {
                let Some(index) = self.index_of(&id) else {
                    return;
                };
                let widget = &self.draft.widgets[index];
                if let Some(size) = stepped_size(&widget.kind, &widget.size, up) {
                    self.apply(EditAction::Resize {
                        id,
                        size: size.to_owned(),
                    });
                }
            }
            EditAction::UpdateCfg { id, cfg, coalesce } => {
                let Some(index) = self.index_of(&id) else {
                    return;
                };
                let mut next = self.draft.widgets.clone();
                let target = &mut next[index].cfg;
                if !target.is_object() {
                    *target = Value::Object(Map::new());
                }
                if let Some(object) = target.as_object_mut() {
                    for (key, value) in cfg {
                        object.insert(key, value);
                    }
                }
                self.commit(next, coalesce.map(|key| format!("{id}:{key}")));
            }
            EditAction::Undo => {
                let Some(previous) = self.past.pop() else {
                    return;
                };
                let current = std::mem::replace(&mut self.draft, previous);
                self.future.insert(0, current);
                self.future.truncate(MAX_HISTORY);
                self.last_coalesce = None;
                self.grabbed = None;
                self.selected = Self::keep_selection(&self.draft, self.selected.take());
            }
            EditAction::Redo => {
                if self.future.is_empty() {
                    return;
                }
                let next = self.future.remove(0);
                let current = std::mem::replace(&mut self.draft, next);
                self.past.push(current);
                if self.past.len() > MAX_HISTORY {
                    self.past.remove(0);
                }
                self.last_coalesce = None;
                self.grabbed = None;
                self.selected = Self::keep_selection(&self.draft, self.selected.take());
            }
            EditAction::Discard => {
                let preview = self.preview;
                *self = Self::new(self.saved.clone(), self.version);
                self.preview = preview;
            }
            EditAction::Saved { board, version } => {
                // History stays, so a save never costs the undo stack.
                self.saved = board.clone();
                self.version = version;
                self.draft = board;
                self.last_coalesce = None;
                self.selected = Self::keep_selection(&self.draft, self.selected.take());
                self.grabbed = None;
            }
            EditAction::Select(id) => {
                self.selected = id;
                self.last_coalesce = None;
            }
            EditAction::Grab(id) => {
                if id.is_some() {
                    self.selected.clone_from(&id);
                }
                self.grabbed = id;
                self.last_coalesce = None;
            }
            EditAction::Preview(preview) => self.preview = preview,
        }
    }
}

/// `steppedSize`: the next size up or down among the type's sizes.
pub fn stepped_size(kind: &str, size: &str, up: bool) -> Option<&'static str> {
    let sizes = spec_of(kind)?.sizes;
    let at = sizes.iter().position(|s| *s == size)?;
    if up {
        sizes.get(at + 1).copied()
    } else {
        at.checked_sub(1).and_then(|i| sizes.get(i).copied())
    }
}

/// `SIZE_LABELS`.
pub fn size_label(size: &str) -> &'static str {
    match size {
        "S" => "Small",
        "L" => "Large",
        _ => "Medium",
    }
}

// ── Readouts ───────────────────────────────────────────────────────────────

/// `formatHours`: "41.5h", "212h", "45m".
pub fn format_hours(seconds: f64) -> String {
    let s = seconds.max(0.0);
    if s < 3600.0 {
        return format!("{}m", (s / 60.0).round() as i64);
    }
    let h = s / 3600.0;
    if h < 100.0 {
        let text = format!("{h:.1}");
        format!("{}h", text.strip_suffix(".0").unwrap_or(&text))
    } else {
        format!("{}h", h.round() as i64)
    }
}

/// `formatDuration`: "11h 20m", "45m", "2h".
pub fn format_duration(seconds: f64) -> String {
    let total = (seconds.max(0.0) / 60.0).round() as i64;
    let (h, m) = (total / 60, total % 60);
    match (h, m) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m:02}m"),
    }
}

/// `formatAgo`: "now", "5m ago", "2h ago", "3d ago", "2w ago", "4mo ago";
/// empty when `iso` does not parse.
pub fn format_ago(iso: &str, now_ms: i64) -> String {
    let Some(at) = parse_rfc3339(iso) else {
        return String::new();
    };
    let minutes = (now_ms - at).div_euclid(60_000);
    if minutes < 1 {
        return "now".into();
    }
    if minutes < 60 {
        return format!("{minutes}m ago");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h ago");
    }
    let days = hours / 24;
    if days < 14 {
        format!("{days}d ago")
    } else if days < 60 {
        format!("{}w ago", days / 7)
    } else {
        format!("{}mo ago", days / 30)
    }
}

/// `formatAge`: the guestbook's compact age ("2d", "5h", "now").
pub fn format_age(iso: &str, now_ms: i64) -> String {
    let ago = format_ago(iso, now_ms);
    ago.strip_suffix(" ago").map(str::to_owned).unwrap_or(ago)
}

const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const LONG_MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn local_parts(at: i64, offset: LocalOffset) -> (i64, i64, i64, i64, i64, usize) {
    let day = local_day(at, offset);
    let (year, month, date) = civil_date(day);
    let local = at + i64::from(offset.at(at)) * 1000;
    let minutes = local.rem_euclid(86_400_000) / 60_000;
    let weekday = (day + 4).rem_euclid(7) as usize;
    (year, month, date, minutes / 60, minutes % 60, weekday)
}

/// `formatDateTime`: "Sat 14 Nov, 21:00" in local time.
pub fn format_date_time(iso: &str, offset: LocalOffset) -> String {
    let Some(at) = parse_rfc3339(iso) else {
        return String::new();
    };
    let (_, month, date, hour, minute, weekday) = local_parts(at, offset);
    format!(
        "{} {date} {}, {hour:02}:{minute:02}",
        DAYS[weekday],
        MONTHS[(month - 1) as usize]
    )
}

/// `formatDayMonth`: "14 Nov".
pub fn format_day_month(iso: &str, offset: LocalOffset) -> String {
    parse_rfc3339(iso).map_or_else(String::new, |at| {
        let (_, month, date, ..) = local_parts(at, offset);
        format!("{date} {}", MONTHS[(month - 1) as usize])
    })
}

/// `formatMonth`: "November".
pub fn format_month(iso: &str, offset: LocalOffset) -> String {
    parse_rfc3339(iso).map_or_else(String::new, |at| {
        let (_, month, ..) = local_parts(at, offset);
        LONG_MONTHS[(month - 1) as usize].to_owned()
    })
}

/// `countdownParts`: `{passed, days, hours, minutes}` left until `iso`.
pub fn countdown_parts(iso: &str, now_ms: i64) -> Value {
    let Some(at) = parse_rfc3339(iso) else {
        return Value::Null;
    };
    let minutes = ((at - now_ms).max(0)) / 60_000;
    json!({
        "passed": at <= now_ms,
        "days": minutes / 1440,
        "hours": minutes % 1440 / 60,
        "minutes": minutes % 60,
    })
}

/// A local "YYYY-MM-DD HH:MM" (the countdown's field) as UTC ISO, or
/// `None` when it is not one.
pub fn local_input_to_iso(text: &str, offset: LocalOffset) -> Option<String> {
    let text = text.trim();
    let (date, time) = text.split_once([' ', 'T']).unwrap_or((text, "00:00"));
    let time = if time.matches(':').count() == 1 {
        format!("{time}:00")
    } else {
        time.to_owned()
    };
    let naive = parse_rfc3339(&format!("{date}T{time}Z"))?;
    // The offset at the guessed instant; good away from a DST switch.
    let utc = naive - i64::from(offset.at(naive)) * 1000;
    Some(crate::store::time::format_rfc3339(utc))
}

/// An ISO time as the countdown's field shows it: local "YYYY-MM-DD HH:MM".
pub fn iso_to_local_input(iso: &str, offset: LocalOffset) -> String {
    parse_rfc3339(iso).map_or_else(String::new, |at| {
        let (year, month, date, hour, minute, _) = local_parts(at, offset);
        format!("{year:04}-{month:02}-{date:02} {hour:02}:{minute:02}")
    })
}

/// `linkHost`: "kiwi.page" for "https://www.kiwi.page/art".
pub fn link_host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    host.strip_prefix("www.")
        .unwrap_or(host)
        .to_ascii_lowercase()
}

/// `guestbookDenialCopy`: why the viewer may not sign, from the server's
/// `denial` (`GUESTBOOK_NO_SHARED_SPACE` and older servers alike get the
/// shared-space line).
pub fn guestbook_denial_copy(denial: Option<&str>, owner_name: &str) -> String {
    match denial {
        Some("GUESTBOOK_OFF") => format!("{owner_name} closed their guestbook."),
        Some("GUESTBOOK_BLOCKED") => format!("You can't sign {owner_name}'s guestbook."),
        Some("GUESTBOOK_OWN") => "This is your guestbook.".to_owned(),
        _ => format!(
            "Only people who share a server or a DM with {owner_name} can sign. Join one of their servers or message them first."
        ),
    }
}

/// `rankName` (Deadlock): "Emissary IV" for badge 74, "Unranked" for 0.
pub fn deadlock_rank_name(badge: i64) -> String {
    const TIERS: [&str; 12] = [
        "Obscurus",
        "Initiate",
        "Seeker",
        "Acolyte",
        "Sentinel",
        "Mystic",
        "Ritualist",
        "Emissary",
        "Oracle",
        "Phantom",
        "Ascendant",
        "Eternus",
    ];
    const ROMAN: [&str; 7] = ["", "I", "II", "III", "IV", "V", "VI"];
    let (tier, sub) = (badge / 10, badge % 10);
    if badge <= 0 || tier < 1 || tier >= TIERS.len() as i64 {
        return "Unranked".into();
    }
    if (1..=6).contains(&sub) {
        format!("{} {}", TIERS[tier as usize], ROMAN[sub as usize])
    } else {
        TIERS[tier as usize].into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widget(id: &str, kind: &str, size: &str) -> Widget {
        Widget {
            id: id.into(),
            kind: kind.into(),
            size: size.into(),
            cfg: default_config(kind),
        }
    }

    fn board(widgets: Vec<Widget>) -> Board {
        Board { v: 1, widgets }
    }

    fn ids(editor: &Editor) -> Vec<&str> {
        editor.draft.widgets.iter().map(|w| w.id.as_str()).collect()
    }

    #[test]
    fn specs_cover_every_type_with_a_valid_default_size() {
        assert_eq!(SPECS.len(), 14);
        for spec in SPECS {
            assert!(spec.sizes.contains(&spec.default_size), "{}", spec.kind);
            assert!(CATEGORIES.iter().any(|(c, _)| *c == spec.category));
        }
        assert!(spec_of("custom").unwrap().perk);
        assert_eq!(SPECS.iter().filter(|s| s.perk).count(), 1);
        assert_eq!(spec_of("guestbook").unwrap().sizes, ["M", "L"]);
    }

    #[test]
    fn the_rack_packs_dense_like_the_web() {
        // M, S, S, L, S: the two S fill beside the M, L goes under, S after.
        let (cells, rows) = pack_rack(["M", "S", "S", "L", "S"]);
        let at: Vec<_> = cells
            .iter()
            .map(|c| (c.col, c.row, c.cols, c.rows))
            .collect();
        assert_eq!(
            at,
            [
                (0, 0, 1, 2),
                (1, 0, 1, 1),
                (1, 1, 1, 1),
                (0, 2, 2, 1),
                (0, 3, 1, 1)
            ]
        );
        assert_eq!(rows, 4);
        // A later S fills a hole an earlier L left.
        let (cells, rows) = pack_rack(["S", "L", "S"]);
        assert_eq!((cells[2].col, cells[2].row), (1, 0));
        assert_eq!(rows, 2);
        assert_eq!(pack_rack([]).1, 0);
    }

    #[test]
    fn caps_limit_types_and_the_board() {
        let mut b = board(vec![widget("a", "links", "S"), widget("b", "links", "S")]);
        assert_eq!(remaining_of_type(&b, "links"), 0);
        assert_eq!(remaining_of_type(&b, "custom"), 2);
        b.widgets = (0..12)
            .map(|i| widget(&format!("w{i}"), "mood", "S"))
            .collect();
        assert_eq!(remaining_of_type(&b, "countdown"), 0);
        assert_eq!(remaining_of_type(&b, "nope"), 0);
    }

    #[test]
    fn widget_ids_look_like_the_web_and_vary() {
        let a = widget_id(1);
        assert_eq!(a.len(), 10);
        assert!(a
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()));
        assert_ne!(a, widget_id(2));
    }

    #[test]
    fn add_selects_and_respects_caps() {
        let mut editor = Editor::new(board(vec![widget("a", "mood", "S")]), 3);
        editor.apply(EditAction::Add {
            kind: "mood".into(),
            index: None,
            seed: 1,
        });
        assert_eq!(editor.draft.widgets.len(), 1, "mood is one per board");
        editor.apply(EditAction::Add {
            kind: "photos".into(),
            index: Some(0),
            seed: 1,
        });
        assert_eq!(editor.draft.widgets[0].kind, "photos");
        assert_eq!(editor.draft.widgets[0].size, "M");
        assert_eq!(
            editor.selected.as_deref(),
            Some(editor.draft.widgets[0].id.as_str())
        );
        assert!(editor.dirty() && editor.can_undo());
    }

    #[test]
    fn remove_moves_selection_to_the_neighbour_and_undo_brings_it_back() {
        let mut editor = Editor::new(
            board(vec![
                widget("a", "mood", "S"),
                widget("b", "links", "S"),
                widget("c", "quote", "L"),
            ]),
            1,
        );
        editor.apply(EditAction::Select(Some("b".into())));
        editor.apply(EditAction::Remove("b".into()));
        assert_eq!(ids(&editor), ["a", "c"]);
        assert_eq!(editor.selected.as_deref(), Some("c"));
        editor.apply(EditAction::Undo);
        assert_eq!(ids(&editor), ["a", "b", "c"]);
        editor.apply(EditAction::Redo);
        assert_eq!(ids(&editor), ["a", "c"]);
    }

    #[test]
    fn move_and_move_by_clamp() {
        let mut editor = Editor::new(
            board(vec![
                widget("a", "mood", "S"),
                widget("b", "links", "S"),
                widget("c", "quote", "L"),
            ]),
            1,
        );
        editor.apply(EditAction::MoveBy {
            id: "a".into(),
            delta: 1,
        });
        assert_eq!(ids(&editor), ["b", "a", "c"]);
        editor.apply(EditAction::MoveBy {
            id: "a".into(),
            delta: -5,
        });
        assert_eq!(ids(&editor), ["a", "b", "c"]);
        editor.apply(EditAction::Move {
            id: "a".into(),
            index: 99,
        });
        assert_eq!(ids(&editor), ["b", "c", "a"]);
    }

    #[test]
    fn resize_only_takes_the_types_sizes() {
        let mut editor = Editor::new(board(vec![widget("g", "guestbook", "M")]), 1);
        editor.apply(EditAction::Resize {
            id: "g".into(),
            size: "S".into(),
        });
        assert!(!editor.dirty());
        editor.apply(EditAction::CycleSize {
            id: "g".into(),
            up: true,
        });
        assert_eq!(editor.draft.widgets[0].size, "L");
        editor.apply(EditAction::CycleSize {
            id: "g".into(),
            up: true,
        });
        assert_eq!(editor.draft.widgets[0].size, "L");
        assert_eq!(stepped_size("guestbook", "M", false), None);
    }

    #[test]
    fn a_typing_run_is_one_undo_step() {
        let mut editor = Editor::new(board(vec![widget("m", "mood", "S")]), 1);
        for text in ["h", "hi", "hi!"] {
            let mut cfg = Map::new();
            cfg.insert("text".into(), json!(text));
            editor.apply(EditAction::UpdateCfg {
                id: "m".into(),
                cfg,
                coalesce: Some("text".into()),
            });
        }
        assert_eq!(editor.past.len(), 1);
        assert_eq!(editor.draft.widgets[0].cfg["text"], "hi!");
        assert_eq!(
            editor.draft.widgets[0].cfg["pose"], "bark",
            "other keys stay"
        );
        editor.apply(EditAction::Undo);
        assert_eq!(editor.draft.widgets[0].cfg["text"], "");
    }

    #[test]
    fn saved_keeps_history_and_discard_resets() {
        let mut editor = Editor::new(board(vec![widget("m", "mood", "S")]), 1);
        editor.apply(EditAction::Remove("m".into()));
        let draft = editor.draft.clone();
        editor.apply(EditAction::Saved {
            board: draft,
            version: 2,
        });
        assert!(!editor.dirty() && editor.can_undo());
        assert_eq!(editor.version, 2);
        editor.apply(EditAction::Undo);
        assert!(editor.dirty());
        editor.apply(EditAction::Preview(Preview::Anyone));
        editor.apply(EditAction::Discard);
        assert!(!editor.dirty() && !editor.can_undo());
        assert_eq!(editor.preview, Preview::Anyone);
    }

    #[test]
    fn readouts_match_board_format() {
        assert_eq!(format_hours(45.0 * 60.0), "45m");
        assert_eq!(format_hours(41.5 * 3600.0), "41.5h");
        assert_eq!(format_hours(2.0 * 3600.0), "2h");
        assert_eq!(format_hours(212.4 * 3600.0), "212h");
        assert_eq!(format_duration(11.0 * 3600.0 + 20.0 * 60.0), "11h 20m");
        assert_eq!(format_duration(3.0 * 3600.0 + 5.0 * 60.0), "3h 05m");
        assert_eq!(format_duration(7200.0), "2h");
        assert_eq!(format_duration(-5.0), "0m");
        let now = parse_rfc3339("2026-11-27T20:30:00Z").unwrap();
        assert_eq!(format_ago("2026-11-27T20:29:40Z", now), "now");
        assert_eq!(format_ago("2026-11-27T20:25:00Z", now), "5m ago");
        assert_eq!(format_ago("2026-11-25T20:30:00Z", now), "2d ago");
        assert_eq!(format_ago("2026-11-01T20:30:00Z", now), "3w ago");
        assert_eq!(format_ago("2026-06-01T20:30:00Z", now), "5mo ago");
        assert_eq!(format_age("2026-11-27T18:30:00Z", now), "2h");
        assert_eq!(format_ago("garbage", now), "");
    }

    #[test]
    fn dates_read_in_local_time() {
        let cst = LocalOffset(|_| -6 * 3600);
        assert_eq!(
            format_date_time("2026-11-15T03:00:00Z", cst),
            "Sat 14 Nov, 21:00"
        );
        assert_eq!(format_day_month("2026-11-15T03:00:00Z", cst), "14 Nov");
        assert_eq!(format_month("2026-12-01T03:00:00Z", cst), "November");
        assert_eq!(
            local_input_to_iso("2026-11-14 21:00", cst).as_deref(),
            Some("2026-11-15T03:00:00.000Z")
        );
        assert_eq!(
            iso_to_local_input("2026-11-15T03:00:00Z", cst),
            "2026-11-14 21:00"
        );
        assert_eq!(local_input_to_iso("soon", cst), None);
    }

    #[test]
    fn countdown_parts_count_down_to_the_minute() {
        let now = parse_rfc3339("2026-11-27T20:30:00Z").unwrap();
        let parts = countdown_parts("2026-11-29T21:00:00Z", now);
        assert_eq!(
            parts,
            json!({ "passed": false, "days": 2, "hours": 0, "minutes": 30 })
        );
        assert_eq!(countdown_parts("2026-11-01T00:00:00Z", now)["passed"], true);
    }

    #[test]
    fn guestbook_denials_read_like_the_web() {
        assert_eq!(
            guestbook_denial_copy(Some("GUESTBOOK_OFF"), "Kiwi"),
            "Kiwi closed their guestbook."
        );
        assert_eq!(
            guestbook_denial_copy(Some("GUESTBOOK_BLOCKED"), "Kiwi"),
            "You can't sign Kiwi's guestbook."
        );
        assert_eq!(
            guestbook_denial_copy(Some("GUESTBOOK_OWN"), "Kiwi"),
            "This is your guestbook."
        );
        let shared = guestbook_denial_copy(None, "Kiwi");
        assert!(shared.starts_with("Only people who share a server or a DM with Kiwi"));
        assert_eq!(
            guestbook_denial_copy(Some("GUESTBOOK_NO_SHARED_SPACE"), "Kiwi"),
            shared
        );
    }

    #[test]
    fn names_hosts_and_ranks() {
        let mut custom = widget("c", "custom", "L");
        assert_eq!(widget_name(&custom), "Custom");
        custom.cfg["legend"] = json!("Commissions");
        assert_eq!(widget_name(&custom), "Commissions");
        assert_eq!(link_host("https://www.Kiwi.page/art?x=1"), "kiwi.page");
        assert_eq!(deadlock_rank_name(74), "Emissary IV");
        assert_eq!(deadlock_rank_name(70), "Emissary");
        assert_eq!(deadlock_rank_name(0), "Unranked");
    }
}
