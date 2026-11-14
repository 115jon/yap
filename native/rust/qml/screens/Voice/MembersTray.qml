import QtQuick
import com.yap.native
import Yap.Ui

// Everyone in the call as a strip above the stage bar while a tile is on
// stage (VoiceStage.tsx MembersTray): 144 px 16:9 thumbnails, 8 px apart,
// centred while they fit; it scrolls once they do not. The tile on stage
// keeps its avatar and takes the accent edge.
Item {
    id: root

    implicitHeight: 81 + 16 + 1

    ListView {
        id: list
        anchors.top: parent.top
        anchors.topMargin: 8
        anchors.horizontalCenter: parent.horizontalCenter
        width: Math.min(parent.width - 32, contentWidth)
        height: 81
        orientation: ListView.Horizontal
        spacing: 8
        clip: contentWidth > width
        interactive: contentWidth > width
        boundsBehavior: Flickable.StopAtBounds
        model: VoiceTileModel
        delegate: VoiceTile {
            width: 144
            height: 81
            tray: true
        }
        Accessible.role: Accessible.List
        Accessible.name: qsTr("Participants")
    }

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.line
    }
}
