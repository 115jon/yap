import QtQuick
import com.yap.native

// The reactions under a message (MessageItem.tsx): 28 px chips, 6 px
// apart, wrapping; a 16 px emoji and the count in Martian 11/600. Yours
// are accent-ink on accent-tint, the rest line-strong and ink-2 (ink-2
// and ink-1 on hover). The add key comes last and shows on row hover.
// `chips` is the row's `reactions` role, parsed.
Flow {
    id: root

    required property var chips
    property bool rowHovered: false

    signal toggled(string emoji)
    signal addRequested(Item anchor)
    // A right click on a chip: who reacted with it.
    signal detailsRequested(string emoji)

    spacing: 6
    topPadding: 6

    Repeater {
        model: root.chips
        Rectangle {
            id: chip
            required property var modelData
            readonly property bool mine: modelData.mine
            readonly property color ink: mine ? Theme.accentInk : chipHover.containsMouse ? Theme.ink1 : Theme.ink2
            width: chipRow.implicitWidth + 16
            height: 28
            radius: Theme.radius
            color: mine ? Theme.accentTint : "transparent"
            border.width: 1
            border.color: mine ? Theme.accentInk : chipHover.containsMouse ? Theme.ink2 : Theme.lineStrong
            Accessible.role: Accessible.Button
            Accessible.name: (mine ? qsTr("Remove %1 reaction, %2") : qsTr("Add %1 reaction, %2"))
                .arg(modelData.label).arg(modelData.count)

            Row {
                id: chipRow
                anchors.centerIn: parent
                spacing: 6
                Image {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: chip.modelData.url.length > 0 && status !== Image.Error
                    width: 16
                    height: 16
                    sourceSize: Qt.size(32, 32)
                    asynchronous: true
                    source: chip.modelData.url
                    smooth: true
                    mipmap: true
                }
                // A custom emoji whose picture is gone: its `:code:`.
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: chip.modelData.url.length === 0
                    width: Math.min(implicitWidth, 80)
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                    text: chip.modelData.label
                    color: chip.ink
                    font: Theme.type.caption
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: chip.modelData.count
                    color: chip.ink
                    font: Theme.type.kbd
                }
            }
            Tooltip {
                visible: chipHover.containsMouse
                text: chip.modelData.tip
            }
            MouseArea {
                id: chipHover
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                onClicked: mouse => {
                    if (mouse.button === Qt.RightButton)
                        root.detailsRequested(chip.modelData.emoji)
                    else
                        root.toggled(chip.modelData.emoji)
                }
            }
        }
    }

    IconKey {
        id: addKey
        visible: root.rowHovered
        size: 28
        tone: "muted"
        iconName: "react"
        tip: qsTr("Add reaction")
        Accessible.name: qsTr("Add another reaction")
        onClicked: root.addRequested(addKey)
    }
}
