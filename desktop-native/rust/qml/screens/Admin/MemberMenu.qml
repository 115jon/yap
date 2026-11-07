import QtQuick
import com.yap.native

// A member's right-click menu in the member panel (MemberList.tsx
// handleMemberContext): Profile (the full profile), Message, Start a call
// (not while that call rings or runs), Copy ID ("User ID copied"), then for
// someone else Ban (with Ban members) behind its confirm. No Kick: the
// web's desktop menus dropped it (only its phone profile sheet has one).
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

    MenuRow {
        text: qsTr("Profile")
        glyph: "account"
        onTriggered: ShellState.showFullProfile(menu.userId, menu.anchorItem)
    }
    MenuRow {
        visible: menu.other
        height: visible ? implicitHeight : 0
        text: qsTr("Message")
        glyph: "chat"
        onTriggered: Nav.openDm(menu.userId)
    }
    MenuRow {
        readonly property var call: JSON.parse(Calls.view)
        visible: menu.other && !((call.status === "ringing_outgoing" && call.remote && call.remote.id === menu.userId)
            || (call.joined === true && call.remote && call.remote.id === menu.userId))
        height: visible ? implicitHeight : 0
        text: qsTr("Start a call")
        glyph: "call"
        onTriggered: ShellState.startCallWithRequested(menu.userId, menu.name)
    }
    MenuRow {
        text: qsTr("Copy ID")
        glyph: "copy"
        onTriggered: {
            clipboard.copyText(menu.userId)
            Chat.notice(qsTr("User ID copied"))
        }
    }
    MenuRule { visible: menu.other && Admin.canBan; height: visible ? implicitHeight : 0 }
    MenuRow {
        visible: menu.other && Admin.canBan
        height: visible ? implicitHeight : 0
        text: qsTr("Ban")
        glyph: "ban"
        danger: true
        onTriggered: ShellState.banRequested(menu.userId, menu.name)
    }
}
