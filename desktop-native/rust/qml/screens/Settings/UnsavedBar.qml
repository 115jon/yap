import QtQuick
import com.yap.native

// The one save pattern for settings that don't save on change
// (UnsavedBar.tsx): hidden while clean, it slides up with a fade over dur-3
// when the form turns dirty. 56 high on ply-3 under a 2 px ink-1 rule: an
// idle "Unsaved changes" LED word (or the error, in danger ink), then the
// rule key Reset and the primary Save changes. `nudge()` shakes it
// (±4 px, three cycles, 240 ms) when a close is refused; reduced motion
// flashes the warning tint instead.
Rectangle {
    id: bar

    property bool dirty: false
    property bool saving: false
    property string error
    property bool saveDisabled: false
    property string saveLabel: qsTr("Save changes")

    signal reset()
    signal save()

    function nudge() {
        if (Theme.reducedMotion)
            flash.restart()
        else
            shake.restart()
    }

    readonly property bool shown: dirty || saving

    width: parent ? parent.width : 0
    height: shown ? 56 : 0
    clip: true
    color: Theme.ply3
    Accessible.role: Accessible.Grouping
    Accessible.name: qsTr("Unsaved changes")
    Behavior on height { NumberAnimation { duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }

    Rectangle {
        id: tintFlash
        anchors.fill: parent
        color: Theme.warningTint
        opacity: 0
    }
    SequentialAnimation {
        id: flash
        OpacityAnimator { target: tintFlash; from: 1; to: 1; duration: 400 }
        OpacityAnimator { target: tintFlash; to: 0; duration: Theme.dur2 }
    }

    Rectangle {
        width: parent.width
        height: 2
        color: Theme.ink1
    }

    Item {
        id: content
        y: 2
        width: parent.width
        height: 54
        enabled: bar.shown
        visible: bar.height > 0
        opacity: bar.shown ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Theme.dur3 } }
        transform: Translate { id: nudgeShift; x: 0 }

        Item {
            anchors.left: parent.left
            anchors.leftMargin: 32
            anchors.right: keys.left
            anchors.rightMargin: 12
            height: parent.height
            LedLabel {
                visible: bar.error.length === 0
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Unsaved changes")
                ledState: "idle"
            }
            Text {
                visible: bar.error.length > 0
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width
                textFormat: Text.PlainText
                text: bar.error
                elide: Text.ElideRight
                color: Theme.dangerInk
                font: Theme.type.description
                Accessible.role: Accessible.AlertMessage
                Accessible.name: text
            }
        }
        Row {
            id: keys
            anchors.right: parent.right
            anchors.rightMargin: 32
            anchors.verticalCenter: parent.verticalCenter
            spacing: 12
            KitButton {
                variant: "rule"
                text: qsTr("Reset")
                enabled: !bar.saving
                onClicked: bar.reset()
            }
            KitButton {
                variant: "primary"
                ground: Theme.ply3
                text: bar.saveLabel
                loading: bar.saving
                enabled: !bar.saveDisabled
                onClicked: if (!bar.saving) bar.save()
            }
        }
    }

    SequentialAnimation {
        id: shake
        loops: 1
        NumberAnimation { target: nudgeShift; property: "x"; to: -4; duration: 34; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeInOut }
        NumberAnimation { target: nudgeShift; property: "x"; to: 4; duration: 34 }
        NumberAnimation { target: nudgeShift; property: "x"; to: -4; duration: 34 }
        NumberAnimation { target: nudgeShift; property: "x"; to: 4; duration: 34 }
        NumberAnimation { target: nudgeShift; property: "x"; to: -4; duration: 34 }
        NumberAnimation { target: nudgeShift; property: "x"; to: 0; duration: 34 }
    }
}
