import QtQuick
import com.yap.native
import Yap.Ui

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

    // Motion (MessageItem.tsx): a reaction added after the strip has
    // painted enters (rm-list-enter); taking back your own last one shrinks
    // it to 0.9 and fades it (100 ms) before the toggle goes out. Chips are
    // made again whenever `chips` changes, so the emoji already shown are
    // remembered and only a new one plays.
    property var shown: ({})
    property bool painted: false
    Component.onCompleted: remember()
    onChipsChanged: Qt.callLater(remember)
    function remember() {
        const next = {}
        for (const chip of root.chips)
            next[chip.emoji] = true
        root.shown = next
        root.painted = true
    }

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
            EaseColor on color {}
            EaseColor on border.color {}
            // PRESS: 0.97 while held.
            scale: chipHover.pressed ? Theme.scalePress : 1
            EaseNumber on scale {}
            transform: Translate { id: lift }
            Component.onCompleted: if (root.painted && root.shown[modelData.emoji] !== true) entrance.start()
            ListEnter {
                id: entrance
                target: chip
                lift: lift
            }
            ParallelAnimation {
                id: leave
                NumberAnimation { target: chip; property: "opacity"; to: 0; duration: Theme.dur2Exit; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
                NumberAnimation { target: chip; property: "scale"; to: Theme.scaleLeave; duration: Theme.dur2Exit; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
                onFinished: {
                    chip.toggleSent = true
                    root.toggled(chip.modelData.emoji)
                    restore.start()
                }
            }
            // The strip rebuilt (someone else reacted) during the leave: the
            // click still counts.
            property bool toggleSent: false
            Component.onDestruction: if (leave.running && !toggleSent) root.toggled(modelData.emoji)
            // The update normally rebuilds the strip first; if the reaction
            // stays (the toggle failed), the chip comes back.
            Timer {
                id: restore
                interval: 400
                onTriggered: {
                    chip.opacity = 1
                    chip.scale = Qt.binding(() => chipHover.pressed ? Theme.scalePress : 1)
                }
            }
            Accessible.role: Accessible.Button
            Accessible.name: (mine ? qsTr("Remove %1 reaction, %2") : qsTr("Add %1 reaction, %2"))
                .arg(modelData.label).arg(modelData.count)

            // The hover and press ink (PRESS: currentColor at 8 / 14 %).
            Rectangle {
                anchors.fill: parent
                radius: parent.radius
                color: chip.ink
                opacity: chipHover.pressed ? 0.14 : chipHover.containsMouse ? 0.08 : 0
                EaseNumber on opacity {}
            }

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
            LazyPopup {
                when: chipHover.containsMouse
                sourceComponent: Tooltip {
                    visible: chipHover.containsMouse
                    text: chip.modelData.tip
                }
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
                    else if (chip.mine && chip.modelData.count <= 1 && !leave.running)
                        leave.start()
                    else if (!leave.running)
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
