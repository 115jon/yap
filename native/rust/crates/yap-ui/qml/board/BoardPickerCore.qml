import QtQuick
import Yap.Ui

// The widget catalogue as both apps offer it (WidgetPicker.tsx): the
// widgets of a group, whether one can still be added, and the tag and
// spoken name each row carries. A type at its cap says "Added" (or "2 of
// 2"), a full board says so, and Custom carries the Loud lock on a free
// plan: it can still be added, as a draft only you see, as on the web.
// The desktop's BoardPicker popover and the phone's catalogue sheet draw
// their own rows over it.
QtObject {
    id: core

    property var catalog: []
    property var categories: []
    property int used: 0
    property int max: 12

    readonly property bool full: used >= max
    // Some widget needs Loud (the footnote under the list).
    readonly property bool anyLocked: catalog.some(item => item.locked)

    function itemsOf(categoryId) {
        return catalog.filter(item => item.category === categoryId)
    }
    // Its type is at its cap (or the board is full).
    function capped(item) {
        return item.remaining === 0
    }
    function canAdd(item) {
        return !capped(item)
    }
    // The row's tag: "Full", "Added", "2 of 2", "Loud" or nothing.
    function tag(item) {
        return capped(item) ? (full ? qsTr("Full") : item.max > 1 ? qsTr("%1 of %1").arg(item.max) : qsTr("Added"))
            : item.locked ? qsTr("Loud") : ""
    }
    function spokenName(item) {
        return item.name + (item.locked ? qsTr(", Loud") : "") + (capped(item) ? (full ? qsTr(", board is full") : qsTr(", added")) : "")
    }
}
