import QtQuick
import QtQuick.Templates as T
import com.yap.native

// The input sensitivity meter (InputSensitivityMeter.tsx, MicTestWidget):
// the mic level as a ladder of 16 cells on tint-3, lit talk above the
// marker and ink-3 below it, and the marker itself, a 2 px warning-ink
// bar. The value is the marker on the 0..100 scale (dB + 100); dragging it
// is the slider, enabled only for a manual threshold.
T.Slider {
    id: control

    // 0..100, as Audio.level.
    property real level: 0
    property bool metering: false
    readonly property int cells: 16
    readonly property int lit: metering ? Math.round(level / 100 * cells) : 0
    readonly property bool above: metering && level >= value

    from: 0
    to: 100
    stepSize: 1
    implicitWidth: 320
    implicitHeight: 24
    focusPolicy: Qt.StrongFocus
    Accessible.role: Accessible.Slider
    Accessible.name: qsTr("Input sensitivity")
    Accessible.description: !metering ? qsTr("Mic level, idle")
        : above ? qsTr("Mic level, people can hear you")
        : qsTr("Mic level, below the sensitivity marker")

    background: Row {
        y: (control.height - height) / 2
        width: control.width
        height: 12
        spacing: 3
        Repeater {
            model: control.cells
            Rectangle {
                required property int index
                width: (control.width - 3 * (control.cells - 1)) / control.cells
                height: 12
                color: index >= control.lit ? Theme.tint3 : control.above ? Theme.talk : Theme.ink3
            }
        }
    }

    handle: Rectangle {
        x: control.visualPosition * (control.width - width)
        y: (control.height - height) / 2
        width: 2
        height: 20
        color: Theme.warningInk
        opacity: control.enabled ? 1 : 0.6
        Rectangle {
            anchors.centerIn: parent
            width: 10
            height: 26
            visible: control.visualFocus
            color: "transparent"
            radius: Theme.radius
            border.width: 2
            border.color: Theme.focus
        }
    }
}
