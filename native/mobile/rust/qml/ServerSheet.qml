import QtQuick
import Yap.Mobile
import Yap.Ui

// The open server's menu (a tap on its name over the tree), or a channel's
// (a long press on its row), as the Expo app's ServerSheet: Invite people
// makes a link (a week, no use limit, for the server; the channel's 7-day
// link for a channel) and hands it to the system share sheet; Mark as read;
// and a channel's Copy link. A link that could not be made says why under
// its row and the sheet stays up. The server's menu also has Create
// channel (with Manage channels: CreateChannelPage) and, for members but
// not the owner (ChannelSidebar.tsx: the owner can't leave), Leave server,
// confirmed in place; the sheet closes once the server is left, or says
// why it could not be.
BottomSheet {
    id: sheet

    // Empty: the server's menu.
    property string channelId: ""
    property string channelName: ""
    property bool voice: false
    readonly property bool onChannel: channelId.length > 0
    property string serverId: ""
    property string serverName: ""
    // Leave server asked: the confirm shows in place of the rows.
    property bool leaving: false
    readonly property var leaveAct: JSON.parse(Admin.state).acts["leave-server"] ?? { busy: false, error: "", done: 0 }
    // The leave's finished count when it was asked for.
    property int leaveDone: 0
    onLeaveActChanged: {
        if (leaving && leaveAct.done > leaveDone)
            close()
    }

    function openServer() {
        channelId = ""
        channelName = ""
        voice = false
        start()
    }
    function openChannel(id, name, isVoice) {
        channelId = id
        channelName = name
        voice = isVoice
        start()
    }
    function start() {
        serverId = Nav.activeServerId
        serverName = Nav.activeServerName
        leaving = false
        maker.reset()
        open()
    }
    function askLeave() {
        Admin.clear("leave-server")
        leaveDone = leaveAct.done
        leaving = true
    }

    title: onChannel ? (voice ? channelName : "#" + channelName) : serverName
    contentHeight: column.implicitHeight
    onClosed: {
        maker.reset()
        leaving = false
    }

    InviteMaker {
        id: maker
        onMade: url => {
            sheet.close()
            Native.share(url, qsTr("Invite people to %1").arg(sheet.serverName))
        }
        onFailed: Native.haptic(Native.Reject)
    }

    Column {
        id: column
        width: parent.width
        bottomPadding: 8

        SheetRow {
            visible: Admin.canCreateInvite && !sheet.leaving
            iconName: "invite"
            text: maker.busy ? qsTr("Making a link…")
                : sheet.onChannel ? qsTr("Invite to channel") : qsTr("Invite people")
            onTriggered: {
                if (sheet.onChannel)
                    maker.makeChannel(sheet.serverId, sheet.channelId)
                else
                    maker.makeServer(maker.weekHours, 0)
            }
        }
        Text {
            visible: maker.error.length > 0
            x: 16 + Theme.iconLg + 16
            width: parent.width - x - 16
            bottomPadding: 8
            text: maker.error
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: Theme.dangerInk
            font: Theme.type.caption
        }
        SheetRow {
            visible: !sheet.leaving
            iconName: "markRead"
            text: qsTr("Mark as read")
            onTriggered: {
                sheet.close()
                if (sheet.onChannel)
                    Chat.markChannelRead(sheet.channelId)
                else
                    Chat.markServerRead(sheet.serverId)
            }
        }
        SheetRow {
            visible: sheet.onChannel
            iconName: "link"
            text: qsTr("Copy link")
            onTriggered: {
                sheet.close()
                Native.copy(Admin.webOrigin + "/chat/" + sheet.serverId + "/" + sheet.channelId)
            }
        }
        SheetRow {
            visible: !sheet.onChannel && !sheet.leaving && Admin.canManageChannels
            iconName: "add"
            text: qsTr("Create channel")
            onTriggered: {
                sheet.close()
                Router.open("CreateChannelPage", { categoryId: "" })
            }
        }
        SheetRow {
            visible: !sheet.onChannel && !sheet.leaving && !Admin.isOwner
            iconName: "logout"
            text: qsTr("Leave server")
            danger: true
            onTriggered: sheet.askLeave()
        }
        Item {
            visible: sheet.leaving
            x: 16
            width: parent.width - 32
            height: visible ? leaveConfirm.implicitHeight + 8 : 0
            InlineConfirm {
                id: leaveConfirm
                consequence: qsTr("You will leave %1 and need a new invite to rejoin.").arg(sheet.serverName)
                confirmLabel: qsTr("Leave server")
                irreversible: true
                busy: sheet.leaveAct.busy
                error: sheet.leaveAct.error
                onConfirmed: Admin.leaveServer(sheet.serverId)
                onCancelled: sheet.leaving = false
            }
        }
    }
}
