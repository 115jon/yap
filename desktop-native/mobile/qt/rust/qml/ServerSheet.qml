import QtQuick
import Yap.Mobile
import Yap.Ui

// The open server's menu (a tap on its name over the tree), or a channel's
// (a long press on its row), as the Expo app's ServerSheet: Invite people
// makes a link (a week, no use limit, for the server; the channel's 7-day
// link for a channel) and hands it to the system share sheet; Mark as read;
// and a channel's Copy link. A link that could not be made says why under
// its row and the sheet stays up.
BottomSheet {
    id: sheet

    // Empty: the server's menu.
    property string channelId: ""
    property string channelName: ""
    property bool voice: false
    readonly property bool onChannel: channelId.length > 0
    property string serverId: ""
    property string serverName: ""

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
        maker.reset()
        open()
    }

    title: onChannel ? (voice ? channelName : "#" + channelName) : serverName
    contentHeight: column.implicitHeight
    onClosed: maker.reset()

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
            visible: Admin.canCreateInvite
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
    }
}
