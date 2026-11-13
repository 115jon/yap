import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The call in progress, along the drawer above your account: Yap.Ui's
// CallStatusLine (the talk LED, the call's state in silkscreen, the channel).
// A tap opens the call page, where its controls are (no second dock here).
Rectangle {
    id: root

    signal callRequested()

    // The window's insets where the drawer meets its edges (the drawer
    // passes its own).
    property real leftInset: 0
    property real rightInset: 0

    readonly property bool shown: Voice.channelId.length > 0
    height: shown ? Math.max(48, line.implicitHeight + 16) : 0
    visible: shown
    color: Theme.ply2
    Accessible.role: Accessible.Button
    Accessible.name: qsTr("%1, %2. Open the call").arg(line.word).arg(Voice.channelName)
    Accessible.onPressAction: root.callRequested()

    Rectangle {
        anchors.fill: parent
        color: tap.pressed ? Theme.tint1 : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.dur1 } }
    }
    Rectangle {
        width: parent.width
        height: 1
        color: Theme.line
    }
    CallStatusLine {
        id: line
        anchors.left: parent.left
        // AccountBar's columns: the LED at 27, the text at 60.
        anchors.leftMargin: 12 + 15 + root.leftInset
        anchors.right: glyphs.left
        anchors.rightMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        clockShown: false
        gap: 12 + 15
        placeUnderWord: true
    }
    Row {
        id: glyphs
        anchors.right: parent.right
        anchors.rightMargin: 18 + root.rightInset
        anchors.verticalCenter: parent.verticalCenter
        spacing: 8
        Icon {
            visible: Voice.muted && !Voice.deafened
            name: "mic"
            variant: "off"
            size: Theme.iconMd
            color: Theme.mutedState
        }
        Icon {
            visible: Voice.deafened
            name: "deafen"
            variant: "off"
            size: Theme.iconMd
            color: Theme.mutedState
        }
        Icon {
            name: "chevronRight"
            size: Theme.iconMd
            color: Theme.ink3
        }
    }
    TapHandler {
        id: tap
        onTapped: root.callRequested()
    }
}
