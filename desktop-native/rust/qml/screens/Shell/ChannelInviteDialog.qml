pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// Invite friends to a channel (ChannelInviteModal.tsx), the kit's C1
// plate: "Invite · Server", "Invite friends to Server", the dithered rule,
// "They land in #channel." A channel-scoped 7-day link is made each time it
// opens. The friends (accepted ones, searchable) can be marked invited; the
// link is a legend plate under them, then Done and Copy link (Copied for
// 2 s).
PlateDialog {
    id: dialog

    property string serverId
    property string serverName
    property string channelId
    property string channelName
    property bool voice: false
    property string query: ""
    property var invited: ({})
    property bool copied: false

    readonly property var invite: JSON.parse(Extras.invite)
    readonly property var friends: JSON.parse(Friends.state).all ?? []
    readonly property var shown: {
        const q = query.trim().toLowerCase()
        return q.length === 0 ? friends
            : friends.filter(f => f.username.toLowerCase().includes(q) || f.name.toLowerCase().includes(q))
    }

    function start(channelId, name, voice) {
        dialog.serverId = Nav.activeServerId
        dialog.serverName = Nav.activeServerName
        dialog.channelId = channelId
        dialog.channelName = name
        dialog.voice = voice
        dialog.query = ""
        dialog.invited = {}
        dialog.copied = false
        Friends.refresh()
        Extras.channelInvite(Nav.activeServerId, channelId)
        open()
    }

    kicker: qsTr("Invite · %1").arg(serverName)
    title: qsTr("Invite friends to %1").arg(serverName)
    rule: true
    description: qsTr("They land in %1.").arg(voice ? channelName : "#" + channelName)
    onClosed: Extras.closeInvite()

    Timer {
        id: copiedTimer
        interval: 2000
        onTriggered: dialog.copied = false
    }
    ClipboardText { id: clipboard }

    FieldPlate {
        width: parent.width
        label: qsTr("Search friends")
        ground: Theme.ply3
        KitField {
            large: true
            leadingInset: 28
            placeholderText: qsTr("Search for friends")
            text: dialog.query
            onTextEdited: dialog.query = text
            Icon {
                x: 16
                anchors.verticalCenter: parent.verticalCenter
                name: "search"
                size: Theme.iconSm
                color: Theme.ink3
            }
        }
    }

    Item {
        width: parent.width
        height: Math.min(240, Math.max(list.contentHeight, 72))
        ListView {
            id: list
            anchors.fill: parent
            clip: true
            spacing: 1
            boundsBehavior: Flickable.StopAtBounds
            model: dialog.shown
            delegate: FriendLine {}
            ScrollBar.vertical: ThinScrollBar {}
        }
        Text {
            anchors.centerIn: parent
            width: parent.width
            visible: list.count === 0
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: dialog.friends.length === 0 ? qsTr("No friends to invite yet. Send the link below instead.")
                : qsTr("No friends match “%1”.").arg(dialog.query)
            color: Theme.ink2
            font: Theme.type.description
        }
    }

    Column {
        width: parent.width
        spacing: 6
        FieldPlate {
            width: parent.width
            label: qsTr("Invite link")
            ground: Theme.ply3
            KitField {
                large: true
                readOnly: true
                text: dialog.invite.loading ? qsTr("Generating…") : dialog.invite.url
                Accessible.name: qsTr("Invite link")
                onActiveFocusChanged: if (activeFocus) selectAll()
            }
        }
        Text {
            width: parent.width
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: dialog.invite.error.length > 0 ? dialog.invite.error
                : dialog.invite.code.length > 0 ? qsTr("Your invite link expires in 7 days.") : ""
            color: dialog.invite.error.length > 0 ? Theme.dangerInk : Theme.ink3
            font: Theme.type.caption
        }
    }

    footer: [
        KitButton {
            variant: "ghost"
            text: qsTr("Done")
            onClicked: dialog.close()
        },
        KitButton {
            variant: "primary"
            ground: Theme.ply3
            iconName: dialog.copied ? "check" : "copy"
            text: dialog.copied ? qsTr("Copied") : qsTr("Copy link")
            loading: dialog.invite.loading
            enabled: !dialog.invite.loading && dialog.invite.code.length > 0
            onClicked: {
                clipboard.copyText(dialog.invite.url)
                dialog.copied = true
                copiedTimer.restart()
            }
        }
    ]

    component FriendLine: Item {
        id: line
        required property var modelData
        readonly property bool sent: dialog.invited[modelData.userId] === true
        width: ListView.view.width
        height: 48
        HoverHandler { id: lineHover }
        Rectangle {
            anchors.fill: parent
            radius: Theme.radius
            color: lineHover.hovered ? Theme.tint1 : "transparent"
            EaseColor on color {}
        }
        Avatar {
            id: face
            x: 8
            anchors.verticalCenter: parent.verticalCenter
            size: 32
            initials: line.modelData.initials
            source: line.modelData.avatar
        }
        Text {
            anchors.left: face.right
            anchors.leftMargin: 12
            anchors.right: key.left
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            elide: Text.ElideRight
            textFormat: Text.PlainText
            text: line.modelData.name
            color: Theme.ink1
            font: Theme.type.row
        }
        KitButton {
            id: key
            anchors.right: parent.right
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            size: "sm"
            variant: line.sent ? "ghost" : "secondary"
            text: line.sent ? qsTr("Invited") : qsTr("Invite")
            enabled: !line.sent
            // As the web does: marked invited (no DM is sent).
            onClicked: {
                const next = Object.assign({}, dialog.invited)
                next[line.modelData.userId] = true
                dialog.invited = next
            }
        }
    }
}
