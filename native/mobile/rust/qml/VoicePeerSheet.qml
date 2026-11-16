import QtQuick
import Yap.Mobile
import Yap.Ui

// Someone in a voice channel, from a long press on their row in the
// drawer's tree (MemberRow, through ShellState.peerMenuRequested): their
// Profile and, for a moderator, "Move to" the server's other voice
// channels (MoveTargetRows).
BottomSheet {
    id: sheet

    property string userId: ""
    property string name: ""
    property string channelId: ""

    signal profileRequested(string userId)

    function show(userId, name, channelId) {
        sheet.userId = userId
        sheet.name = name
        sheet.channelId = channelId
        sheet.open()
    }

    title: name
    contentHeight: column.implicitHeight

    Column {
        id: column
        width: parent.width
        bottomPadding: 8

        SheetRow {
            text: qsTr("Profile")
            iconName: "profile"
            onTriggered: {
                sheet.close()
                sheet.profileRequested(sheet.userId)
            }
        }
        MoveTargetRows {
            userId: sheet.userId
            channelId: sheet.channelId
            shown: sheet.up
            onMoved: sheet.close()
        }
    }
}
