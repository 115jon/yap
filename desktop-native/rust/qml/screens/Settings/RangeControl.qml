import QtQuick
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// A value fader (primitives.tsx RangeControl, ui/slider.tsx): an 8 px
// tint-3 track whose range is the accent dither ramp, a 12 x 28 ink-1
// thumb, and the Martian readout ("80%") at the end. `moved` carries the
// whole value; the row names it (`Accessible.name`).
Item {
    id: root

    property real value: 0
    property real from: 0
    property real to: 100
    property string unit: "%"
    property alias accessibleName: slider.accessibleName

    signal moved(real value)

    implicitWidth: 320
    implicitHeight: 32

    T.Slider {
        id: slider
        HoverHandler { cursorShape: Qt.PointingHandCursor }

        property string accessibleName

        anchors.left: parent.left
        anchors.right: readout.left
        anchors.rightMargin: 16
        height: parent.height
        from: root.from
        to: root.to
        stepSize: 1
        snapMode: T.Slider.SnapAlways
        focusPolicy: Qt.StrongFocus
        opacity: enabled ? 1 : 0.45
        onMoved: root.moved(Math.round(value))

        Accessible.role: Accessible.Slider
        Accessible.name: accessibleName

        background: Rectangle {
            x: slider.leftPadding
            y: (slider.height - height) / 2
            width: slider.availableWidth
            height: 8
            radius: 1
            color: Theme.tint3
            clip: true
            Image {
                width: slider.visualPosition * parent.width
                height: parent.height
                smooth: false
                fillMode: Image.Stretch
                source: "image://dither/ramp/30/100/0/" + Theme.hex(Theme.accentInk) + "/64/4"
            }
        }

        handle: Rectangle {
            x: slider.leftPadding + slider.visualPosition * (slider.availableWidth - width)
            y: (slider.height - height) / 2
            width: 12
            height: 28
            radius: Theme.radius
            color: Theme.ink1
            scale: slider.pressed && !Theme.reducedMotion ? 1.1 : 1
            Behavior on scale { NumberAnimation { duration: Theme.dur1 } }
            Rectangle {
                anchors.fill: parent
                anchors.margins: -4
                visible: slider.visualFocus
                color: "transparent"
                radius: Theme.radius
                border.width: 2
                border.color: Theme.focus
            }
        }
    }

    // The store's value, except while the thumb is held.
    Binding {
        target: slider
        property: "value"
        value: root.value
        when: !slider.pressed
        restoreMode: Binding.RestoreNone
    }

    Text {
        id: readout
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        width: 48
        horizontalAlignment: Text.AlignRight
        textFormat: Text.PlainText
        text: Math.round(slider.value) + root.unit
        color: root.enabled ? Theme.ink1 : Theme.ink3
        font: Theme.type.num
        Accessible.ignored: true
    }
}
