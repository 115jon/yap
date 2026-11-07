import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// The incoming call card (IncomingCallModal.tsx): a 320 float at the top
// right with no scrim, shown while a call rings for us and its DM is not
// the open one (there the DM's call area answers). The caller as a tile
// (avatar with the ringing ring, Bricolage 18/800 name, the "Incoming
// voice call" LED word), then Decline (danger) and Accept (talk green).
// Focus lands on Accept; Escape declines. Accept opens the DM.
Popup {
    id: root

    readonly property var calls: JSON.parse(Calls.view)
    readonly property bool ringing: calls.status === "ringing_incoming"
    readonly property bool wanted: ringing && calls.channelId !== Nav.activeChannelId

    onWantedChanged: wanted ? open() : close()
    // Made as the call rings (LazyPopup).
    Component.onCompleted: if (wanted) open()

    parent: Overlay.overlay
    x: parent ? parent.width - width - 16 : 0
    y: 48
    width: 320
    padding: 0
    modal: false
    focus: true
    closePolicy: Popup.NoAutoClose
    onOpened: acceptKey.forceActiveFocus()

    background: Rectangle {
        radius: 2
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        FloatShadow {}
        Rectangle { width: parent.width; height: 2; color: Theme.ink1 }
    }
    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 200; easing.type: Easing.OutCubic }
            NumberAnimation { property: "y"; from: 40; to: 48; duration: 200; easing.type: Easing.OutCubic }
        }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: Theme.dur3Exit }
    }

    contentItem: Column {
        Keys.onEscapePressed: Calls.decline()
        Rectangle {
            width: root.width
            height: 72
            color: Theme.ply2
            Accessible.role: Accessible.AlertMessage
            Accessible.name: qsTr("Incoming voice call from %1").arg(root.calls.remote.name)
            Row {
                x: 16
                anchors.verticalCenter: parent.verticalCenter
                spacing: 12
                Item {
                    width: 40
                    height: 40
                    anchors.verticalCenter: parent.verticalCenter
                    Avatar {
                        id: face
                        size: 40
                        initials: root.calls.remote.name ?? ""
                        source: root.calls.remote.avatar ?? ""
                    }
                    Rectangle {
                        anchors.centerIn: face
                        width: 48
                        height: 48
                        radius: Theme.radius + 2
                        color: "transparent"
                        border.width: 2
                        border.color: Theme.talk
                        SequentialAnimation on opacity {
                            running: root.opened && !Theme.reducedMotion
                            loops: Animation.Infinite
                            OpacityAnimator { from: 1; to: 0.15; duration: 900; easing.type: Easing.InOutSine }
                            OpacityAnimator { from: 0.15; to: 1; duration: 900; easing.type: Easing.InOutSine }
                        }
                    }
                }
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 2
                    Text {
                        width: root.width - 100
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                        text: root.calls.remote.name ?? ""
                        color: Theme.ink1
                        font.family: "Yap Bricolage Key ExtraBold"
                        font.pixelSize: 18
                        font.weight: Font.ExtraBold
                        font.letterSpacing: -0.36
                    }
                    LedLabel {
                        text: qsTr("Incoming voice call")
                        ledState: "idle"
                    }
                }
            }
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line }
        }
        Row {
            padding: 12
            spacing: 8
            KitButton {
                width: (root.width - 32) / 2
                variant: "danger"
                iconName: "disconnect"
                text: qsTr("Decline")
                onClicked: Calls.decline()
            }
            KitButton {
                id: acceptKey
                width: (root.width - 32) / 2
                variant: "talk"
                iconName: "call"
                text: qsTr("Accept")
                onClicked: Calls.accept(false)
            }
        }
    }
}
