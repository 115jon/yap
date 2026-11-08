import QtQuick
import QtQuick.Templates as T
import com.yap.native

// One source in the share picker (DesktopScreenPickerUI.tsx SourceGrid): a
// ply-2 tile with a line edge, its still letterboxed on tint-1 over a 44 px
// label row (the window's app icon, or the monitor or window glyph, then
// the name). Selected draws a 2 px accent-ink edge over the line and a check
// badge; there is no accent fill. Until the still arrives, or when there is
// none, the glyph stands in for it.
T.AbstractButton {
    id: root

    required property string sourceId
    required property string name
    required property string kind
    required property bool ours

    property bool selected: false
    // Stills and icons only of our own windows (YAP_SHARE_ONLY_OURS).
    property bool stillAllowed: true
    // Bumped by the picker on each open, so stills are taken afresh.
    property int nonce: 0

    readonly property bool monitor: kind === "monitor"
    readonly property real thumbHeight: Math.round(width * 9 / 16)

    implicitHeight: thumbHeight + 44
    focusPolicy: Qt.StrongFocus
    Accessible.role: Accessible.RadioButton
    Accessible.checkable: true
    Accessible.checked: selected
    Accessible.name: name

    background: Rectangle {
        radius: Theme.radius
        color: Theme.ply2
        border.width: 1
        border.color: root.selected ? "transparent" : root.hovered ? Theme.lineStrong : Theme.line
        EaseColor on border.color {}
    }

    contentItem: Item {
        // The still: object-contain on tint-1, clipped to the tile's top.
        Rectangle {
            id: thumb
            x: 1
            y: 1
            width: parent.width - 2
            height: root.thumbHeight - 1
            color: Theme.tint1
            radius: Theme.radius
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: Theme.radius
                color: Theme.tint1
            }
            Image {
                id: still
                anchors.fill: parent
                visible: status === Image.Ready
                asynchronous: true
                cache: false
                fillMode: Image.PreserveAspectFit
                sourceSize.width: 480
                source: root.stillAllowed ? "image://share/thumb/" + root.sourceId + "/" + root.nonce : ""
            }
            Icon {
                anchors.centerIn: parent
                visible: still.status !== Image.Ready
                name: root.monitor ? "monitor" : "window"
                size: Theme.iconXl
                color: Theme.ink3
            }
        }

        Row {
            x: 12
            y: root.thumbHeight
            height: 44
            width: parent.width - 24
            spacing: 8
            Item {
                anchors.verticalCenter: parent.verticalCenter
                width: 16
                height: 16
                Image {
                    id: appIcon
                    anchors.fill: parent
                    visible: status === Image.Ready
                    asynchronous: true
                    sourceSize.width: 16
                    sourceSize.height: 16
                    source: root.monitor || !root.stillAllowed ? "" : "image://share/icon/" + root.sourceId
                    Accessible.ignored: true
                }
                Icon {
                    anchors.centerIn: parent
                    visible: appIcon.status !== Image.Ready
                    name: root.monitor ? "monitor" : "window"
                    size: Theme.iconSm
                    color: Theme.ink3
                }
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - 24
                textFormat: Text.PlainText
                text: root.name
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.row
            }
        }

        // Selected: the 2 px accent-ink edge over the line, and the check.
        Rectangle {
            anchors.fill: parent
            anchors.margins: -root.padding
            visible: root.selected
            color: "transparent"
            radius: Theme.radius
            border.width: 2
            border.color: Theme.accentInk
        }
        Rectangle {
            visible: root.selected
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 8
            width: 24
            height: 24
            radius: 12
            color: Theme.ply3
            border.width: 1
            border.color: Theme.line
            Icon {
                anchors.centerIn: parent
                name: "check"
                size: Theme.iconXs
                color: Theme.accentInk
            }
        }
        // Keyboard focus: the 2 px ring outside the tile.
        Rectangle {
            anchors.fill: parent
            anchors.margins: -root.padding - 3
            visible: root.visualFocus
            color: "transparent"
            radius: Theme.radius
            border.width: 2
            border.color: Theme.focus
        }
    }

    HoverHandler { cursorShape: Qt.PointingHandCursor }
}
