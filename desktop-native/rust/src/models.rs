//! The list models: `QAbstractListModel` subclasses (cxx-qt's custom base
//! class pattern), registered as QML singletons. Rows and role names come
//! from `yap_core::store::view`; the rows themselves live in [`crate::gui`]
//! on the GUI thread, so `data()` is a borrow and a match, never a parse.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++Qt" {
        include!(<QtCore/QAbstractListModel>);
        #[qobject]
        type QAbstractListModel;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;

        include!("cxx-qt-lib/qvector.h");
        type QVector_i32 = cxx_qt_lib::QVector<i32>;
    }

    extern "RustQt" {
        /// The server rail.
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qml_singleton]
        type ServerListModel = super::ServerListModelRust;

        /// The flattened channel tree: labels, channels, call members.
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qml_singleton]
        type ChannelTreeModel = super::ChannelTreeModelRust;

        /// The open channel's messages, oldest first.
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qml_singleton]
        type MessageListModel = super::MessageListModelRust;

        /// Everyone in the current call.
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qml_singleton]
        type VoiceRosterModel = super::VoiceRosterModelRust;

        /// The composer's `@` suggestions.
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qml_singleton]
        type MentionListModel = super::MentionListModelRust;
    }

    impl cxx_qt::Initialize for ServerListModel {}
    impl cxx_qt::Initialize for ChannelTreeModel {}
    impl cxx_qt::Initialize for MessageListModel {}
    impl cxx_qt::Initialize for VoiceRosterModel {}
    impl cxx_qt::Initialize for MentionListModel {}

    // Messages: scroll anchoring around appends.
    extern "RustQt" {
        /// Right before rows are appended at the end (read `atYEnd` here).
        #[qsignal]
        #[cxx_name = "aboutToAppend"]
        fn about_to_append(self: Pin<&mut MessageListModel>);

        /// Rows were appended at the end; `mine` if any is the user's own.
        #[qsignal]
        fn appended(self: Pin<&mut MessageListModel>, mine: bool);
    }

    // The same inherited and overridden members on every model.

    unsafe extern "RustQt" {
        #[inherit]
        #[qsignal]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut ServerListModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );
        #[inherit]
        #[qsignal]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut ChannelTreeModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );
        #[inherit]
        #[qsignal]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut MessageListModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );
        #[inherit]
        #[qsignal]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut VoiceRosterModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );
        #[inherit]
        #[qsignal]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut MentionListModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );
    }

    unsafe extern "RustQt" {
        #[inherit]
        fn index(
            self: &ServerListModel,
            row: i32,
            column: i32,
            parent: &QModelIndex,
        ) -> QModelIndex;
        #[inherit]
        fn index(
            self: &ChannelTreeModel,
            row: i32,
            column: i32,
            parent: &QModelIndex,
        ) -> QModelIndex;
        #[inherit]
        fn index(
            self: &MessageListModel,
            row: i32,
            column: i32,
            parent: &QModelIndex,
        ) -> QModelIndex;
        #[inherit]
        fn index(
            self: &VoiceRosterModel,
            row: i32,
            column: i32,
            parent: &QModelIndex,
        ) -> QModelIndex;
        #[inherit]
        fn index(
            self: &MentionListModel,
            row: i32,
            column: i32,
            parent: &QModelIndex,
        ) -> QModelIndex;
    }

    // # Safety (all below): each begin_* must be followed by its end_*.
    extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginInsertRows"]
        unsafe fn begin_insert_rows(
            self: Pin<&mut ServerListModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut ServerListModel>);
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(
            self: Pin<&mut ServerListModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut ServerListModel>);
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut ServerListModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut ServerListModel>);

        #[inherit]
        #[cxx_name = "beginInsertRows"]
        unsafe fn begin_insert_rows(
            self: Pin<&mut ChannelTreeModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut ChannelTreeModel>);
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(
            self: Pin<&mut ChannelTreeModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut ChannelTreeModel>);
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut ChannelTreeModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut ChannelTreeModel>);

        #[inherit]
        #[cxx_name = "beginInsertRows"]
        unsafe fn begin_insert_rows(
            self: Pin<&mut MessageListModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut MessageListModel>);
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(
            self: Pin<&mut MessageListModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut MessageListModel>);
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut MessageListModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut MessageListModel>);

        #[inherit]
        #[cxx_name = "beginInsertRows"]
        unsafe fn begin_insert_rows(
            self: Pin<&mut VoiceRosterModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut VoiceRosterModel>);
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(
            self: Pin<&mut VoiceRosterModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut VoiceRosterModel>);
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut VoiceRosterModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut VoiceRosterModel>);

        #[inherit]
        #[cxx_name = "beginInsertRows"]
        unsafe fn begin_insert_rows(
            self: Pin<&mut MentionListModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut MentionListModel>);
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(
            self: Pin<&mut MentionListModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut MentionListModel>);
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut MentionListModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut MentionListModel>);
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &ServerListModel, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &ServerListModel) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &ServerListModel, parent: &QModelIndex) -> i32;

        #[qinvokable]
        #[cxx_override]
        fn data(self: &ChannelTreeModel, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &ChannelTreeModel) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &ChannelTreeModel, parent: &QModelIndex) -> i32;

        #[qinvokable]
        #[cxx_override]
        fn data(self: &MessageListModel, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &MessageListModel) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &MessageListModel, parent: &QModelIndex) -> i32;

        #[qinvokable]
        #[cxx_override]
        fn data(self: &VoiceRosterModel, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &VoiceRosterModel) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &VoiceRosterModel, parent: &QModelIndex) -> i32;

        #[qinvokable]
        #[cxx_override]
        fn data(self: &MentionListModel, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &MentionListModel) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &MentionListModel, parent: &QModelIndex) -> i32;
    }
}

use core::pin::Pin;

use cxx_qt_lib::QVector;
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
use yap_core::store::view::{
    Cell, ChannelTreeRow, MentionRow, MessageRow, Row, ServerRow, VoiceMemberRow,
};
use yap_core::ViewDiff;

use crate::gui::{self, Rows, Slot};

/// Role ids start after `Qt::UserRole` (256).
const FIRST_ROLE: i32 = 257;

fn variant(cell: Cell<'_>) -> QVariant {
    match cell {
        Cell::Str(text) => QVariant::from(&QString::from(text)),
        Cell::Bool(value) => QVariant::from(&value),
        Cell::Int(value) => QVariant::from(&value),
    }
}

fn role_names<R: Row>() -> QHash<QHashPair_i32_QByteArray> {
    let mut names = QHash::<QHashPair_i32_QByteArray>::default();
    for (i, &role) in R::ROLES.iter().enumerate() {
        names.insert(FIRST_ROLE + i as i32, QByteArray::from(R::role_name(role)));
    }
    names
}

fn cell_variant<R: Row>(rows: &[R], index: &QModelIndex, role: i32) -> QVariant {
    let role = usize::try_from(role - FIRST_ROLE)
        .ok()
        .and_then(|i| R::ROLES.get(i));
    let row = usize::try_from(index.row()).ok().and_then(|i| rows.get(i));
    match (row, role) {
        (Some(row), Some(&role)) => variant(row.get(role)),
        _ => QVariant::default(),
    }
}

fn qt_index(i: usize) -> i32 {
    i32::try_from(i).unwrap_or(i32::MAX)
}

/// One model type: its rows in [`Rows`] and the inherited
/// `QAbstractItemModel` calls the diff adapter needs.
pub(crate) trait ListModel: Slot {
    type Row: Row;

    fn rows(rows: &Rows) -> &Vec<Self::Row>;
    fn rows_mut(rows: &mut Rows) -> &mut Vec<Self::Row>;

    /// # Safety
    /// Must be paired with [`Self::end_insert`].
    unsafe fn begin_insert(self: Pin<&mut Self>, first: i32, last: i32);
    /// # Safety
    /// Only after [`Self::begin_insert`].
    unsafe fn end_insert(self: Pin<&mut Self>);
    /// # Safety
    /// Must be paired with [`Self::end_remove`].
    unsafe fn begin_remove(self: Pin<&mut Self>, first: i32, last: i32);
    /// # Safety
    /// Only after [`Self::begin_remove`].
    unsafe fn end_remove(self: Pin<&mut Self>);
    /// # Safety
    /// Must be paired with [`Self::end_reset`].
    unsafe fn begin_reset(self: Pin<&mut Self>);
    /// # Safety
    /// Only after [`Self::begin_reset`].
    unsafe fn end_reset(self: Pin<&mut Self>);
    fn changed(self: Pin<&mut Self>, row: i32, roles: &QVector<i32>);

    /// Hooks around an append at the end (messages only).
    fn before_append(self: Pin<&mut Self>) {}
    fn after_append(self: Pin<&mut Self>, _rows: &[Self::Row]) {}
}

macro_rules! list_model {
    ($model:ident, $rust:ident, $row:ty, $field:ident) => {
        #[derive(Default)]
        pub struct $rust;

        impl Drop for $rust {
            fn drop(&mut self) {
                gui::unregister::<qobject::$model>();
            }
        }

        impl cxx_qt::Initialize for qobject::$model {
            fn initialize(self: Pin<&mut Self>) {
                gui::register(self);
            }
        }

        impl qobject::$model {
            pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
                gui::with_rows(|rows| cell_variant(&rows.$field, index, role))
            }

            pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
                role_names::<$row>()
            }

            pub fn row_count(&self, parent: &QModelIndex) -> i32 {
                if parent.is_valid() {
                    return 0;
                }
                gui::with_rows(|rows| qt_index(rows.$field.len()))
            }
        }
    };
}

macro_rules! list_model_ops {
    ($row:ty, $field:ident) => {
        fn rows(rows: &Rows) -> &Vec<$row> {
            &rows.$field
        }

        fn rows_mut(rows: &mut Rows) -> &mut Vec<$row> {
            &mut rows.$field
        }

        unsafe fn begin_insert(self: Pin<&mut Self>, first: i32, last: i32) {
            unsafe { self.begin_insert_rows(&QModelIndex::default(), first, last) }
        }

        unsafe fn end_insert(self: Pin<&mut Self>) {
            unsafe { self.end_insert_rows() }
        }

        unsafe fn begin_remove(self: Pin<&mut Self>, first: i32, last: i32) {
            unsafe { self.begin_remove_rows(&QModelIndex::default(), first, last) }
        }

        unsafe fn end_remove(self: Pin<&mut Self>) {
            unsafe { self.end_remove_rows() }
        }

        unsafe fn begin_reset(self: Pin<&mut Self>) {
            unsafe { self.begin_reset_model() }
        }

        unsafe fn end_reset(self: Pin<&mut Self>) {
            unsafe { self.end_reset_model() }
        }

        fn changed(self: Pin<&mut Self>, row: i32, roles: &QVector<i32>) {
            let index = self.index(row, 0, &QModelIndex::default());
            self.data_changed(&index, &index, roles);
        }
    };
}

list_model!(ServerListModel, ServerListModelRust, ServerRow, servers);
list_model!(ChannelTreeModel, ChannelTreeModelRust, ChannelTreeRow, tree);
list_model!(MessageListModel, MessageListModelRust, MessageRow, messages);
list_model!(
    VoiceRosterModel,
    VoiceRosterModelRust,
    VoiceMemberRow,
    roster
);
list_model!(MentionListModel, MentionListModelRust, MentionRow, mentions);

impl ListModel for qobject::ServerListModel {
    type Row = ServerRow;
    list_model_ops!(ServerRow, servers);
}

impl ListModel for qobject::ChannelTreeModel {
    type Row = ChannelTreeRow;
    list_model_ops!(ChannelTreeRow, tree);
}

impl ListModel for qobject::VoiceRosterModel {
    type Row = VoiceMemberRow;
    list_model_ops!(VoiceMemberRow, roster);
}

impl ListModel for qobject::MentionListModel {
    type Row = MentionRow;
    list_model_ops!(MentionRow, mentions);
}

impl ListModel for qobject::MessageListModel {
    type Row = MessageRow;
    list_model_ops!(MessageRow, messages);

    fn before_append(self: Pin<&mut Self>) {
        self.about_to_append();
    }

    fn after_append(self: Pin<&mut Self>, rows: &[MessageRow]) {
        self.appended(rows.iter().any(|r| r.mine));
    }
}

/// Applies one diff to a model's rows, with the matching begin/end calls and
/// `dataChanged` for exactly the changed roles. Without a live model object
/// (QML has not touched it yet) only the rows change.
pub(crate) fn apply<M: ListModel>(diff: ViewDiff<M::Row>) {
    let Some(mut model) = gui::object::<M>() else {
        if !gui::with_rows_mut(|rows| diff.apply(M::rows_mut(rows))) {
            eprintln!("[model] dropped a diff that does not fit");
        }
        return;
    };
    let len = gui::with_rows(|rows| M::rows(rows).len());
    match diff {
        ViewDiff::Reset(new) => {
            // SAFETY: paired with end_reset below; the rows change in between.
            unsafe { model.as_mut().begin_reset() };
            gui::with_rows_mut(|rows| *M::rows_mut(rows) = new);
            unsafe { model.as_mut().end_reset() };
        }
        ViewDiff::Insert { at, rows: new } if at <= len && !new.is_empty() => {
            let append = at == len;
            if append {
                model.as_mut().before_append();
            }
            let (first, last) = (qt_index(at), qt_index(at + new.len() - 1));
            // SAFETY: paired with end_insert; the range matches the splice.
            unsafe { model.as_mut().begin_insert(first, last) };
            let appended = append.then(|| new.clone());
            gui::with_rows_mut(|rows| {
                M::rows_mut(rows).splice(at..at, new);
            });
            unsafe { model.as_mut().end_insert() };
            if let Some(appended) = appended {
                model.as_mut().after_append(&appended);
            }
        }
        ViewDiff::Update { at, row, roles } if at < len => {
            gui::with_rows_mut(|rows| M::rows_mut(rows)[at] = row);
            let mut ids = QVector::<i32>::default();
            for role in roles {
                ids.append(FIRST_ROLE + qt_index(<M::Row as Row>::role_index(role)));
            }
            model.as_mut().changed(qt_index(at), &ids);
        }
        ViewDiff::Remove { at, count } if count > 0 && at + count <= len => {
            let (first, last) = (qt_index(at), qt_index(at + count - 1));
            // SAFETY: paired with end_remove; the range matches the drain.
            unsafe { model.as_mut().begin_remove(first, last) };
            gui::with_rows_mut(|rows| {
                M::rows_mut(rows).drain(at..at + count);
            });
            unsafe { model.as_mut().end_remove() };
        }
        ViewDiff::Insert { rows, .. } if rows.is_empty() => {}
        ViewDiff::Remove { count: 0, .. } => {}
        other => eprintln!("[model] dropped a diff that does not fit {len} rows: {other:?}"),
    }
}
