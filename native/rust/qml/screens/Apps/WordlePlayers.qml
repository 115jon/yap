pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// The others in the call who play: their name, streak and each guess as
// coloured squares only (their letters stay hidden).
Column {
    id: root

    property var players: []
    property var markColor: null
    spacing: 16

    Text {
        textFormat: Text.PlainText
        text: qsTr("In this call")
        color: Theme.ink3
        font: Theme.type.captionStrong
    }
    Repeater {
        model: root.players
        delegate: Column {
            id: player
            required property var modelData
            spacing: 4
            Text {
                width: root.width
                textFormat: Text.PlainText
                elide: Text.ElideRight
                text: player.modelData.finished
                    ? (player.modelData.missed ? qsTr("%1 · Missed").arg(player.modelData.name) : qsTr("%1 · Solved").arg(player.modelData.name))
                    : player.modelData.name
                color: Theme.ink1
                font: Theme.type.row
            }
            Repeater {
                model: player.modelData.rows
                delegate: Row {
                    id: marks
                    required property var modelData
                    spacing: 2
                    Repeater {
                        model: marks.modelData
                        delegate: Rectangle {
                            required property string modelData
                            width: 12
                            height: 12
                            radius: 2
                            color: root.markColor ? root.markColor(modelData) : Theme.tint3
                        }
                    }
                }
            }
            Text {
                visible: player.modelData.streak > 0
                textFormat: Text.PlainText
                text: qsTr("Streak %1").arg(player.modelData.streak)
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
    }
}
