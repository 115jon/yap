import QtQuick
import com.yap.native

// The too-big size meter (toobig-spec.md): a 6 px track spanning
// max(size, limit); the part within the limit in the accent ink, the
// overflow a dithered warning stripe, a 2 px marker at the limit in ink 1,
// and under it "{limit} limit" and the size in the warning ink, in the
// mono numeric face. Static: drawn once per size.
Item {
    id: root

    // Bytes; `sizeText` and `limitText` are the card's words for them.
    property real size: 0
    property real limit: 1
    property string sizeText
    property string limitText

    readonly property real span: Math.max(size, limit, 1)
    readonly property real limitX: Math.round(track.width * Math.min(limit, span) / span)

    implicitWidth: 260
    implicitHeight: track.height + 6 + labels.implicitHeight
    Accessible.ignored: true

    Rectangle {
        id: track
        width: parent.width
        height: 6
        radius: 1
        color: Theme.ply3
        Rectangle {
            width: root.limitX
            height: parent.height
            radius: 1
            color: Theme.accentInk
        }
        // The overflow: 2 px cells of a half-lit Bayer tile.
        Image {
            x: root.limitX
            width: Math.max(0, track.width - root.limitX)
            height: parent.height
            visible: width > 0
            smooth: false
            source: width > 0 ? "image://dither/dots/50/" + Theme.hex(Theme.warningInk)
                + "/" + Math.ceil(width / 2) + "/3" : ""
            fillMode: Image.Stretch
        }
        Rectangle {
            x: Math.min(track.width - width, Math.max(0, root.limitX - 1))
            y: -4
            width: 2
            height: parent.height + 8
            color: Theme.ink1
        }
    }
    Item {
        id: labels
        y: track.height + 6
        width: parent.width
        implicitHeight: limitLabel.implicitHeight
        Text {
            id: limitLabel
            textFormat: Text.PlainText
            text: qsTr("%1 limit").arg(root.limitText)
            color: Theme.ink3
            font: Theme.type.numSm
        }
        Text {
            anchors.right: parent.right
            visible: root.sizeText.length > 0
            textFormat: Text.PlainText
            text: root.sizeText
            color: Theme.warningInk
            font: Theme.type.numSm
        }
    }
}
