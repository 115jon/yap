import QtQuick
import com.yap.native

// A member's right-click menu in the member panel (MemberList.tsx): Profile,
// Message, Copy ID, then for someone else Kick (with Kick members; the web
// offers it on the profile sheet) and Ban (with Ban members), each behind
// its confirm.
MenuPanel {
    id: menu

    property string userId
    property string name
    property Item anchorItem: null

    function openFor(id, label, anchor, x, y) {
        userId = id
        name = label
        anchorItem = anchor
        popup(anchor, x, y)
    }

    readonly property bool other: userId !== Session.meId

    implicitWidth: 220

    ClipboardText { id: clipboard }

    MenuLabel { text: menu.name }
    MenuRow {
        text: qsTr("Profile")
        glyph: "account"
        onTriggered: ShellState.showProfile(menu.userId, menu.anchorItem, "left")
    }
    MenuRow {
        visible: menu.other
        height: visible ? implicitHeight : 0
        text: qsTr("Message")
        glyph: "chat"
        onTriggered: Nav.openDm(menu.userId)
    }
    MenuRow {
        text: qsTr("Copy ID")
        glyph: "copy"
        onTriggered: clipboard.copyText(menu.userId)
    }
    MenuRule { visible: menu.other && (Admin.canKick || Admin.canBan); height: visible ? implicitHeight : 0 }
    MenuRow {
        visible: menu.other && Admin.canKick
        height: visible ? implicitHeight : 0
        text: qsTr("Kick")
        glyph: "userRemove"
        danger: true
        onTriggered: ShellState.kickRequested(menu.userId, menu.name)
    }
    MenuRow {
        visible: menu.other && Admin.canBan
        height: visible ? implicitHeight : 0
        text: qsTr("Ban")
        glyph: "ban"
        danger: true
        onTriggered: ShellState.banRequested(menu.userId, menu.name)
    }
}
