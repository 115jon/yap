pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// The profile card's action row (ProfileCard.tsx ProfileActionRow,
// ProfileActionNotices, ProfileBlockConfirm). The primary follows the
// relationship:
//   friend    Message                      more: Remove friend, Block
//   incoming  Accept, then Message and Decline
//   outgoing  Message, then Cancel request
//   none      Message, then Add friend
//   blocked   Unblock, and the blocked notice
// With `callable` (the full profile) a Call key follows the secondaries
// (ProfileActionRow `extra`), never for someone blocked.
// The more key also copies the user ID. Block asks in a strip under the
// keys. While one action runs the others wait; a failure shows its reason
// under the keys (a failed block, in the strip).
Column {
    id: root

    required property string userId
    required property string username
    required property string name
    // The ply behind the card (the notched key's notch).
    property color ground: Theme.ply3
    // Offer Call (the full profile).
    property bool callable: false

    signal messageRequested()
    signal callRequested()

    readonly property var model: JSON.parse(Friends.state)
    readonly property string relation: model.card && model.card.userId === userId ? model.card.relation : "none"
    readonly property var act: model.acts?.[userId] ?? { busy: "", error: "", failed: "", done: 0 }
    readonly property bool pending: act.busy.length > 0
    property bool confirmBlock: false

    function run(action) {
        Friends.act(action, userId, username, name)
    }

    // A fresh card starts clean (the web's hook state is per card).
    onUserIdChanged: {
        confirmBlock = false
        Friends.clear(userId)
    }
    onRelationChanged: if (relation === "blocked") confirmBlock = false

    width: parent ? parent.width : 0
    spacing: 8
    visible: relation !== "self" && userId.length > 0

    ClipboardText { id: clipboard }

    // The primary across the card, then the secondaries and the more key.
    NotchedKey {
        visible: root.relation !== "blocked"
        width: parent.width
        large: false
        ground: root.ground
        enabled: !root.pending
        text: root.relation === "incoming" ? qsTr("Accept") : qsTr("Message")
        iconName: root.relation === "incoming" ? "check" : "chat"
        onClicked: {
            if (root.relation === "incoming")
                root.run("accept")
            else
                root.messageRequested()
        }
    }

    Item {
        width: parent.width
        height: Math.max(keyRow.height, more.height)

        Row {
            id: keyRow
            readonly property bool call: root.callable && root.relation !== "blocked"
            readonly property int keys: (root.relation === "incoming" ? 2 : root.relation === "outgoing" || root.relation === "none" ? 1 : 0)
                + (call ? 1 : 0)
            readonly property real share: keys > 0 ? (width - spacing * (keys - 1)) / keys : 0
            width: parent.width - more.width - 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8

            KitButton {
                visible: root.relation === "incoming"
                width: keyRow.share
                variant: "secondary"
                iconName: "chat"
                text: qsTr("Message")
                enabled: !root.pending
                onClicked: root.messageRequested()
            }
            KitButton {
                visible: root.relation === "incoming"
                width: keyRow.share
                variant: "ghost"
                text: qsTr("Decline")
                loading: root.act.busy === "decline"
                enabled: !root.pending
                onClicked: root.run("decline")
            }
            KitButton {
                visible: root.relation === "outgoing"
                width: keyRow.share
                variant: "secondary"
                text: qsTr("Cancel request")
                loading: root.act.busy === "cancel"
                enabled: !root.pending
                onClicked: root.run("cancel")
            }
            KitButton {
                visible: root.relation === "none"
                width: keyRow.share
                variant: "secondary"
                iconName: "invite"
                text: qsTr("Add friend")
                loading: root.act.busy === "add"
                enabled: !root.pending
                onClicked: root.run("add")
            }
            KitButton {
                visible: keyRow.call
                width: keyRow.share
                variant: "secondary"
                iconName: "call"
                text: qsTr("Call")
                enabled: !root.pending
                onClicked: root.callRequested()
            }
            KitButton {
                visible: root.relation === "blocked"
                variant: "secondary"
                text: qsTr("Unblock")
                loading: root.act.busy === "unblock"
                enabled: !root.pending
                onClicked: root.run("unblock")
            }
        }
        IconKey {
            id: more
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            size: 32
            iconName: "more"
            tone: "rest"
            tip: qsTr("More actions for %1").arg(root.name)
            onClicked: moreMenu.popup(more, 0, more.height + 4)
        }
    }

    // The blocked notice (ProfileNotice).
    Rectangle {
        visible: root.relation === "blocked"
        width: parent.width
        height: notice.implicitHeight + 16
        radius: Theme.radius
        color: Theme.tint1
        Icon {
            x: 12
            y: 9
            name: "ban"
            size: Theme.iconSm
            color: Theme.ink3
        }
        Text {
            id: notice
            x: 36
            y: 8
            width: parent.width - 48
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: qsTr("You blocked %1. They can't message you or send requests.").arg(root.name)
            color: Theme.ink2
            font: Theme.type.description
        }
    }

    RowError {
        above: 0
        text: root.act.failed !== "block" ? root.act.error : ""
    }

    Loader {
        active: root.confirmBlock && root.relation !== "blocked"
        visible: active
        width: parent.width
        sourceComponent: ConfirmStrip {
            consequence: qsTr("Block %1? They'll be removed from your friends and can't send you friend requests. You can unblock them later.").arg(root.name)
            confirmLabel: qsTr("Block %1").arg(root.name)
            irreversible: false
            busy: root.act.busy === "block"
            error: root.act.failed === "block" ? root.act.error : ""
            onConfirmed: root.run("block")
            onCancelled: root.confirmBlock = false
        }
    }

    MenuPanel {
        id: moreMenu
        implicitWidth: 220
        MenuRow {
            text: qsTr("Copy user ID")
            glyph: "copyId"
            onTriggered: clipboard.copyText(root.userId)
        }
        MenuRule {
            visible: root.relation !== "blocked"
            height: visible ? implicitHeight : 0
        }
        MenuRow {
            visible: root.relation === "friend"
            height: visible ? implicitHeight : 0
            text: qsTr("Remove friend")
            glyph: "userRemove"
            danger: true
            onTriggered: root.run("remove")
        }
        MenuRow {
            visible: root.relation !== "blocked"
            height: visible ? implicitHeight : 0
            text: qsTr("Block")
            glyph: "ban"
            danger: true
            onTriggered: root.confirmBlock = true
        }
    }
}
