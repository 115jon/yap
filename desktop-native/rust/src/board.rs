//! `Board`: the full profile's board column ([`yap_core::store::board`]):
//! the open profile's board, its Activity and Wishlist tabs, the guestbook
//! and the owner's edit in place, as five JSON strings so a change to one
//! (a guestbook post) never re-parses the others. It also formats the
//! widgets' readouts ([`yap_core::profile_board`]), so the web's numbers
//! and dates read the same here.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// Loading, tabs, the editor's flags, the guestbook's notices and
        /// the widgets' `ctx` (store/board.rs `board_view`).
        #[qproperty(QString, state)]
        /// `{rows, unit, cells: [{id, type, size, cfg, draft, col, row,
        /// cols, rows, ...}]}`.
        #[qproperty(QString, rack)]
        /// `BoardData`, URLs absolute, plus `badges_view`.
        #[qproperty(QString, data)]
        #[qproperty(QString, activity)]
        #[qproperty(QString, wishlist)]
        type Board = super::BoardRust;

        #[qinvokable]
        fn open(self: &Board, user_id: &QString);
        #[qinvokable]
        fn close(self: &Board);
        #[qinvokable]
        #[cxx_name = "setTab"]
        fn set_tab(self: &Board, tab: &QString);
        #[qinvokable]
        fn retry(self: &Board);

        #[qinvokable]
        #[cxx_name = "activityMore"]
        fn activity_more(self: &Board);
        /// Empty for every kind.
        #[qinvokable]
        #[cxx_name = "activityKind"]
        fn activity_kind(self: &Board, kind: &QString);
        #[qinvokable]
        #[cxx_name = "hideEvent"]
        fn hide_event(self: &Board, id: &QString, hidden: bool);
        #[qinvokable]
        #[cxx_name = "hideKind"]
        fn hide_kind(self: &Board, kind: &QString, hidden: bool);
        #[qinvokable]
        #[cxx_name = "setRetention"]
        fn set_retention(self: &Board, days: i32);

        #[qinvokable]
        fn post(self: &Board, body: &QString);
        #[qinvokable]
        #[cxx_name = "deleteNote"]
        fn delete_note(self: &Board, id: &QString);
        #[qinvokable]
        fn block(self: &Board, user_id: &QString);
        #[qinvokable]
        #[cxx_name = "setGuestbookEnabled"]
        fn set_guestbook_enabled(self: &Board, enabled: bool);

        /// Edit board (true) or leave without saving (false).
        #[qinvokable]
        fn edit(self: &Board, start: bool);
        /// A widget of `kind` at `index` (-1: the end).
        #[qinvokable]
        fn add(self: &Board, kind: &QString, index: i32);
        #[qinvokable]
        fn remove(self: &Board, id: &QString);
        #[qinvokable]
        #[cxx_name = "moveTo"]
        fn move_to(self: &Board, id: &QString, index: i32);
        #[qinvokable]
        #[cxx_name = "moveBy"]
        fn move_by(self: &Board, id: &QString, delta: i32);
        #[qinvokable]
        fn resize(self: &Board, id: &QString, size: &QString);
        #[qinvokable]
        #[cxx_name = "cycleSize"]
        fn cycle_size(self: &Board, id: &QString, up: bool);
        /// One config key, `value` as JSON; `coalesce` for a typing run.
        #[qinvokable]
        #[cxx_name = "setCfg"]
        fn set_cfg(self: &Board, id: &QString, key: &QString, value: &QString, coalesce: bool);
        /// Empty clears the selection.
        #[qinvokable]
        fn select(self: &Board, id: &QString);
        #[qinvokable]
        fn grab(self: &Board, id: &QString);
        #[qinvokable]
        fn undo(self: &Board);
        #[qinvokable]
        fn redo(self: &Board);
        #[qinvokable]
        fn discard(self: &Board);
        #[qinvokable]
        fn done(self: &Board);
        #[qinvokable]
        #[cxx_name = "useTheirs"]
        fn use_theirs(self: &Board);
        #[qinvokable]
        #[cxx_name = "keepMine"]
        fn keep_mine(self: &Board);
        /// `me`, `friend` or `anyone`.
        #[qinvokable]
        #[cxx_name = "setPreview"]
        fn set_preview(self: &Board, audience: &QString);
        /// The wishlist's items as JSON.
        #[qinvokable]
        #[cxx_name = "saveWishlist"]
        fn save_wishlist(self: &Board, items: &QString);

        #[qinvokable]
        fn hours(self: &Board, seconds: f64) -> QString;
        #[qinvokable]
        fn duration(self: &Board, seconds: f64) -> QString;
        #[qinvokable]
        fn ago(self: &Board, iso: &QString) -> QString;
        #[qinvokable]
        fn age(self: &Board, iso: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "dateTime"]
        fn date_time(self: &Board, iso: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "dayMonth"]
        fn day_month(self: &Board, iso: &QString) -> QString;
        #[qinvokable]
        fn month(self: &Board, iso: &QString) -> QString;
        /// `{passed, days, hours, minutes}` as JSON, or `null`.
        #[qinvokable]
        fn countdown(self: &Board, iso: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "rankName"]
        fn rank_name(self: &Board, badge: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "linkHost"]
        fn link_host(self: &Board, url: &QString) -> QString;
        /// Why the viewer may not sign the guestbook (`denial`, may be empty).
        #[qinvokable]
        #[cxx_name = "denialCopy"]
        fn denial_copy(self: &Board, denial: &QString, owner_name: &QString) -> QString;
        /// A local "YYYY-MM-DD HH:MM" as UTC ISO, or empty.
        #[qinvokable]
        #[cxx_name = "localToIso"]
        fn local_to_iso(self: &Board, text: &QString) -> QString;
    }

    impl cxx_qt::Initialize for Board {}
}

use core::pin::Pin;
use std::time::{SystemTime, UNIX_EPOCH};

use cxx_qt_lib::QString;
use yap_core::profile_board::{self as model, EditAction, Preview};
use yap_core::store::board::{cfg_action, BoardCommand, BoardView};
use yap_core::store::Command;

use crate::client;
use crate::gui;

#[derive(Default)]
pub struct BoardRust {
    state: QString,
    rack: QString,
    data: QString,
    activity: QString,
    wishlist: QString,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn send(command: BoardCommand) {
    client::command(Command::Board(command));
}

fn act(action: EditAction) {
    send(BoardCommand::Act(action));
}

fn text(value: &QString) -> String {
    value.to_string()
}

fn optional(value: &QString) -> Option<String> {
    Some(value.to_string()).filter(|v| !v.is_empty())
}

fn apply(mut object: Pin<&mut qobject::Board>, view: &BoardView) {
    // Each setter notifies only on a change.
    object.as_mut().set_state(QString::from(&view.state));
    object.as_mut().set_rack(QString::from(&view.rack));
    object.as_mut().set_data(QString::from(&view.data));
    object.as_mut().set_activity(QString::from(&view.activity));
    object.set_wishlist(QString::from(&view.wishlist));
}

/// The store's board view changed.
pub(crate) fn on_view(view: BoardView) {
    if let Some(object) = gui::object::<qobject::Board>() {
        apply(object, &view);
    }
    gui::keep_board(view);
}

impl gui::Slot for qobject::Board {
    crate::slot_cell!(qobject::Board);

    fn restore(self: Pin<&mut Self>) {
        apply(self, &gui::board_view());
    }
}

impl cxx_qt::Initialize for qobject::Board {
    fn initialize(self: Pin<&mut Self>) {
        gui::register(self);
    }
}

impl Drop for BoardRust {
    fn drop(&mut self) {
        gui::unregister::<qobject::Board>();
    }
}

fn index(value: i32) -> Option<usize> {
    usize::try_from(value).ok()
}

impl qobject::Board {
    pub fn open(&self, user_id: &QString) {
        send(BoardCommand::Open {
            user_id: text(user_id),
            now_ms: now_ms(),
        });
    }

    pub fn close(&self) {
        send(BoardCommand::Close);
    }

    pub fn set_tab(&self, tab: &QString) {
        send(BoardCommand::Tab(text(tab)));
    }

    pub fn retry(&self) {
        send(BoardCommand::Retry);
    }

    pub fn activity_more(&self) {
        send(BoardCommand::ActivityMore);
    }

    pub fn activity_kind(&self, kind: &QString) {
        send(BoardCommand::ActivityKind(optional(kind)));
    }

    pub fn hide_event(&self, id: &QString, hidden: bool) {
        send(BoardCommand::HideEvent {
            id: text(id),
            hidden,
        });
    }

    pub fn hide_kind(&self, kind: &QString, hidden: bool) {
        send(BoardCommand::HideKind {
            kind: text(kind),
            hidden,
        });
    }

    pub fn set_retention(&self, days: i32) {
        send(BoardCommand::Retention(i64::from(days)));
    }

    pub fn post(&self, body: &QString) {
        send(BoardCommand::GuestbookPost {
            body: text(body),
            now_ms: now_ms(),
        });
    }

    pub fn delete_note(&self, id: &QString) {
        send(BoardCommand::GuestbookDelete(text(id)));
    }

    pub fn block(&self, user_id: &QString) {
        send(BoardCommand::GuestbookBlock(text(user_id)));
    }

    pub fn set_guestbook_enabled(&self, enabled: bool) {
        send(BoardCommand::GuestbookEnabled(enabled));
    }

    pub fn edit(&self, start: bool) {
        send(BoardCommand::Edit(start));
    }

    pub fn add(&self, kind: &QString, at: i32) {
        act(EditAction::Add {
            kind: text(kind),
            index: index(at),
            seed: 0,
        });
    }

    pub fn remove(&self, id: &QString) {
        act(EditAction::Remove(text(id)));
    }

    pub fn move_to(&self, id: &QString, at: i32) {
        act(EditAction::Move {
            id: text(id),
            index: index(at).unwrap_or(0),
        });
    }

    pub fn move_by(&self, id: &QString, delta: i32) {
        act(EditAction::MoveBy {
            id: text(id),
            delta: i64::from(delta),
        });
    }

    pub fn resize(&self, id: &QString, size: &QString) {
        act(EditAction::Resize {
            id: text(id),
            size: text(size),
        });
    }

    pub fn cycle_size(&self, id: &QString, up: bool) {
        act(EditAction::CycleSize { id: text(id), up });
    }

    pub fn set_cfg(&self, id: &QString, key: &QString, value: &QString, coalesce: bool) {
        let Ok(value) = serde_json::from_str(&text(value)) else {
            return;
        };
        act(cfg_action(&text(id), &text(key), value, coalesce));
    }

    pub fn select(&self, id: &QString) {
        act(EditAction::Select(optional(id)));
    }

    pub fn grab(&self, id: &QString) {
        act(EditAction::Grab(optional(id)));
    }

    pub fn undo(&self) {
        act(EditAction::Undo);
    }

    pub fn redo(&self) {
        act(EditAction::Redo);
    }

    pub fn discard(&self) {
        act(EditAction::Discard);
    }

    pub fn done(&self) {
        send(BoardCommand::Done);
    }

    pub fn use_theirs(&self) {
        send(BoardCommand::UseTheirs);
    }

    pub fn keep_mine(&self) {
        send(BoardCommand::KeepMine);
    }

    pub fn set_preview(&self, audience: &QString) {
        act(EditAction::Preview(Preview::parse(&text(audience))));
    }

    pub fn save_wishlist(&self, items: &QString) {
        if let Ok(items) = serde_json::from_str(&text(items)) {
            send(BoardCommand::SaveWishlist(items));
        }
    }

    pub fn hours(&self, seconds: f64) -> QString {
        QString::from(&model::format_hours(seconds))
    }

    pub fn duration(&self, seconds: f64) -> QString {
        QString::from(&model::format_duration(seconds))
    }

    pub fn ago(&self, iso: &QString) -> QString {
        QString::from(&model::format_ago(&text(iso), now_ms()))
    }

    pub fn age(&self, iso: &QString) -> QString {
        QString::from(&model::format_age(&text(iso), now_ms()))
    }

    pub fn date_time(&self, iso: &QString) -> QString {
        QString::from(&model::format_date_time(&text(iso), client::LOCAL))
    }

    pub fn day_month(&self, iso: &QString) -> QString {
        QString::from(&model::format_day_month(&text(iso), client::LOCAL))
    }

    pub fn month(&self, iso: &QString) -> QString {
        QString::from(&model::format_month(&text(iso), client::LOCAL))
    }

    pub fn countdown(&self, iso: &QString) -> QString {
        QString::from(&model::countdown_parts(&text(iso), now_ms()).to_string())
    }

    pub fn rank_name(&self, badge: i32) -> QString {
        QString::from(&model::deadlock_rank_name(i64::from(badge)))
    }

    pub fn link_host(&self, url: &QString) -> QString {
        QString::from(&model::link_host(&text(url)))
    }

    pub fn denial_copy(&self, denial: &QString, owner_name: &QString) -> QString {
        let denial = optional(denial);
        QString::from(&model::guestbook_denial_copy(
            denial.as_deref(),
            &text(owner_name),
        ))
    }

    pub fn local_to_iso(&self, value: &QString) -> QString {
        QString::from(&model::local_input_to_iso(&text(value), client::LOCAL).unwrap_or_default())
    }
}
