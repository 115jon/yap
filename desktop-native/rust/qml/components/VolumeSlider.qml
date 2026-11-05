import QtQuick
import QtQuick.Templates as T
import com.yap.native

// The volume slider (AudioDeviceMenu.tsx VolumeSlider, ContextMenuItems
// Slider): the label and the Martian percentage over a 4 px tint-3 track
// with the primary-fill range and a 14 px thumb ringed in the panel ply.
// `moved` carries whole percents.
T.Slider {
    id: control

    property string label

    from: 0
    to: 200
    stepSize: 1
    snapMode: T.Slider.SnapAlways
    implicitWidth: 232
    implicitHeight: 56
    leftPadding: 8
    rightPadding: 8
    topPadding: 30
    bottomPadding: 12
    focusPolicy: Qt.StrongFocus

    Accessible.role: Accessible.Slider
    Accessible.name: label

    Text {
        x: control.leftPadding
        y: 6
        text: control.label
        color: Theme.ink3
        font: Theme.type.captionStrong
    }
    Text {
        anchors.right: parent.right
        anchors.rightMargin: control.rightPadding
        y: 6
        text: qsTr("%1%").arg(Math.round(control.value))
        color: Theme.ink1
        font: Theme.type.numSm
    }

    background: Item {
        x: control.leftPadding
        y: control.topPadding + (control.availableHeight - height) / 2
        width: control.availableWidth
        height: 4
        Rectangle {
            anchors.fill: parent
            radius: 2
            color: Theme.tint3
        }
        Rectangle {
            width: control.handle.x + control.handle.width / 2 - control.leftPadding
            height: parent.height
            radius: 2
            color: Theme.primaryFill
        }
    }

    handle: Rectangle {
        x: control.leftPadding + control.visualPosition * (control.availableWidth - width)
        y: control.topPadding + (control.availableHeight - height) / 2
        width: 14
        height: 14
        radius: 7
        color: Theme.primaryFill
        border.width: 2
        border.color: Theme.ply3
        Rectangle {
            anchors.fill: parent
            anchors.margins: -4
            visible: control.visualFocus
            color: "transparent"
            radius: width / 2
            border.width: 2
            border.color: Theme.focus
        }
    }
}
