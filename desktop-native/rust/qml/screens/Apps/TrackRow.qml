import QtQuick
import com.yap.native

// A track in Listen Together (a search result or a queue entry): artwork,
// title over artist, duration (or a word such as Radio), and the row's keys.
Rectangle {
    id: root

    property string title
    property string artist
    property string artwork
    property real durationMs: 0
    property bool current: false
    // Shown where the running time goes when there is none ("Radio").
    property string tail: ""
    default property alias keys: keyRow.data

    function clock(ms) {
        const total = Math.max(0, Math.floor(ms / 1000))
        return qsTr("%1:%2").arg(Math.floor(total / 60)).arg(String(total % 60).padStart(2, "0"))
    }

    implicitHeight: 52
    radius: Theme.radius
    color: current ? Theme.accentTint : hover.hovered ? Theme.tint1 : "transparent"
    HoverHandler { id: hover }

    Rectangle {
        id: art
        x: 6
        anchors.verticalCenter: parent.verticalCenter
        width: 40
        height: 40
        radius: 4
        color: Theme.tint2
        Image {
            anchors.fill: parent
            visible: root.artwork.length > 0 && status === Image.Ready
            source: root.artwork
            sourceSize.width: 80
            sourceSize.height: 80
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
        }
        Icon {
            visible: root.artwork.length === 0
            anchors.centerIn: parent
            name: "music"
            size: Theme.iconSm
            color: Theme.ink3
        }
    }
    Column {
        anchors.left: art.right
        anchors.leftMargin: 10
        anchors.right: duration.left
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: root.title
            elide: Text.ElideRight
            color: Theme.ink1
            font: Theme.type.row
        }
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: root.artist
            elide: Text.ElideRight
            color: Theme.ink3
            font: Theme.type.caption
        }
    }
    Text {
        id: duration
        anchors.right: keyRow.left
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        textFormat: Text.PlainText
        text: root.durationMs > 0 ? root.clock(root.durationMs) : root.tail
        color: Theme.ink3
        font: Theme.type.numSm
    }
    Row {
        id: keyRow
        anchors.right: parent.right
        anchors.rightMargin: 4
        anchors.verticalCenter: parent.verticalCenter
    }
}
