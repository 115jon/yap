import QtQuick
import Yap.Ui

// SensitiveMediaFrame.tsx: over blurred media, the scrim and a ply-3
// float (line edge) so its text keeps contrast whatever is underneath:
// "Sensitive media" (13/600), "Hidden by your high media filter." (12,
// ink-2, when there is room) and a 28 px tint-2 "Show media" key. It shows
// while `hidden`; the key reveals the attachment for this session.
// `hidden` follows the web's rule: a sensitive picture or video under the
// high media filter (media-safety.ts `shouldBlurSensitiveAttachment`).
Rectangle {
    id: root

    required property string attachmentId
    property bool hidden: false
    property int cornerRadius: 0
    // A frame too narrow for the card and its title (a phone's mosaic tile):
    // the key alone, "Show", on the scrim.
    readonly property bool compact: width < 160

    anchors.fill: parent
    visible: hidden
    radius: cornerRadius
    color: Theme.scrim

    Rectangle {
        anchors.centerIn: parent
        width: root.compact ? show.width : Math.min(parent.width - 24, 240)
        height: card.implicitHeight + (root.compact ? 0 : 24)
        radius: Theme.radiusSheet
        color: root.compact ? "transparent" : Theme.ply3
        border.width: root.compact ? 0 : 1
        border.color: Theme.line
        Column {
            id: card
            anchors.centerIn: parent
            width: root.compact ? show.width : parent.width - 32
            spacing: 8
            Column {
                width: parent.width
                visible: !root.compact
                spacing: 2
                Text {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    elide: Text.ElideRight
                    text: qsTr("Sensitive media")
                    color: Theme.ink1
                    font: Theme.type.panelTitle
                }
                Text {
                    width: parent.width
                    visible: root.height >= 120
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    text: qsTr("Hidden by your high media filter.")
                    color: Theme.ink2
                    font: Theme.type.caption
                }
            }
            Rectangle {
                id: show
                anchors.horizontalCenter: parent.horizontalCenter
                width: showText.implicitWidth + 20
                height: 28
                radius: Theme.radius
                // Alone on the scrim, the key is a ply-3 float of its own.
                color: root.compact ? Theme.ply3 : showHover.containsMouse ? Theme.tint3 : Theme.tint2
                border.width: root.compact ? 1 : 0
                border.color: Theme.line
                EaseColor on color {}
                scale: showHover.pressed ? Theme.scalePress : 1
                activeFocusOnTab: root.hidden
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Show media")
                Keys.onReturnPressed: ShellState.reveal(root.attachmentId)
                Keys.onSpacePressed: ShellState.reveal(root.attachmentId)
                Text {
                    id: showText
                    anchors.centerIn: parent
                    text: root.compact ? qsTr("Show") : qsTr("Show media")
                    color: Theme.ink1
                    font: Theme.type.captionStrong
                }
                Rectangle {
                    anchors.fill: parent
                    anchors.margins: -3
                    visible: show.activeFocus
                    color: "transparent"
                    radius: Theme.radius
                    border.width: 2
                    border.color: Theme.focus
                }
                MouseArea {
                    id: showHover
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: ShellState.reveal(root.attachmentId)
                }
            }
        }
    }
}
