import QtQuick
import QtQuick.Templates as T
import Yap.Ui

// A level, 0 to 100 by default: the label (and an optional description)
// over the range and its number. Saved once the finger rests (250 ms) or
// lifts.
Column {
    id: vol

    property string text
    property string description
    property int value: 0
    property int from: 0
    property int to: 100
    property bool disabled: false

    signal committed(int value)

    width: parent ? parent.width : 0
    topPadding: 8
    bottomPadding: 4
    opacity: disabled ? 0.45 : 1

    Text {
        width: parent.width
        textFormat: Text.PlainText
        text: vol.text
        wrapMode: Text.Wrap
        color: Theme.ink1
        font: Theme.type.setting
    }
    Text {
        width: parent.width
        visible: vol.description.length > 0
        topPadding: 2
        textFormat: Text.PlainText
        text: vol.description
        wrapMode: Text.Wrap
        color: Theme.ink3
        font: Theme.type.caption
    }
    Item {
        width: parent.width
        height: 48
        T.Slider {
            id: range
            anchors.left: parent.left
            anchors.right: volNum.left
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            height: 48
            from: vol.from
            to: vol.to
            stepSize: 1
            snapMode: T.Slider.SnapAlways
            enabled: !vol.disabled
            Accessible.role: Accessible.Slider
            Accessible.name: vol.text
            Accessible.description: Math.round(value) + "%"
            onMoved: rest.restart()
            onPressedChanged: if (!pressed && rest.running) {
                rest.stop()
                vol.committed(Math.round(value))
            }
            Binding on value {
                when: !range.pressed && !rest.running
                value: vol.value
                restoreMode: Binding.RestoreNone
            }
            background: Rectangle {
                x: range.leftPadding
                y: (range.height - height) / 2
                width: range.availableWidth
                height: 6
                radius: 1
                color: Theme.tint3
                Rectangle {
                    width: range.visualPosition * parent.width
                    height: parent.height
                    radius: 1
                    color: Theme.accentInk
                }
            }
            handle: Rectangle {
                x: range.leftPadding + range.visualPosition * (range.availableWidth - width)
                y: (range.height - height) / 2
                width: 14
                height: 30
                radius: Theme.radius
                color: Theme.ink1
            }
            Timer {
                id: rest
                interval: 250
                onTriggered: vol.committed(Math.round(range.value))
            }
            // The page closed inside the wait: keep the last value.
            Component.onDestruction: if (rest.running) vol.committed(Math.round(range.value))
        }
        Text {
            id: volNum
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            width: 36
            horizontalAlignment: Text.AlignRight
            textFormat: Text.PlainText
            text: Math.round(range.value)
            color: Theme.ink2
            font: Theme.type.num
            Accessible.ignored: true
        }
    }
}
