pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// Who watches a stream (StreamWatcherList.tsx): up to four 24 px avatars
// overlapping by 8, "+N" past them, then "3 viewers" over the first two
// names ("Ana, You and 1 more"). A ply-3 float with a line-strong edge.
// `watchers` is the tile's JSON list `[{userId, name, avatar, initials}]`.
Rectangle {
    id: root

    property string watchers: ""
    readonly property var list: watchers.length > 0 ? JSON.parse(watchers) : []
    readonly property var shown: list.slice(0, 4)
    readonly property int overflow: list.length - shown.length
    readonly property string names: {
        const named = list.slice(0, 2).map(w => w.name)
        const rest = list.length - named.length
        return rest > 0 ? qsTr("%1 and %2 more").arg(named.join(", ")).arg(rest) : named.join(", ")
    }

    visible: list.length > 0
    implicitWidth: row.implicitWidth + 20
    implicitHeight: row.implicitHeight + 12
    radius: Theme.radiusSheet * 2
    color: Theme.ply3
    border.width: 1
    border.color: Theme.lineStrong
    Accessible.role: Accessible.StaticText
    Accessible.name: (root.list.length === 1 ? qsTr("1 viewer") : qsTr("%1 viewers").arg(root.list.length)) + ": " + root.names

    Row {
        id: row
        anchors.centerIn: parent
        spacing: 8
        Row {
            anchors.verticalCenter: parent.verticalCenter
            spacing: -8
            Repeater {
                model: root.shown
                Rectangle {
                    id: face
                    required property var modelData
                    width: 28
                    height: 28
                    radius: Theme.radius + 2
                    color: Theme.ply3
                    Avatar {
                        anchors.centerIn: parent
                        size: 24
                        initials: face.modelData.initials
                        source: face.modelData.avatar
                    }
                }
            }
            Rectangle {
                visible: root.overflow > 0
                width: 28
                height: 28
                radius: Theme.radius + 2
                color: Theme.ply3
                Rectangle {
                    anchors.centerIn: parent
                    width: 24
                    height: 24
                    radius: Theme.radius
                    color: Theme.ply4
                    Text {
                        anchors.centerIn: parent
                        text: "+" + root.overflow
                        color: Theme.ink1
                        font: Theme.type.numSm
                    }
                }
            }
        }
        Column {
            anchors.verticalCenter: parent.verticalCenter
            Text {
                text: root.list.length === 1 ? qsTr("1 viewer") : qsTr("%1 viewers").arg(root.list.length)
                color: Theme.ink1
                font: Theme.type.captionStrong
            }
            Text {
                width: Math.min(implicitWidth, 180)
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: root.names
                color: Theme.ink2
                font: Theme.type.caption
            }
        }
    }
}
