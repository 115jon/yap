import QtQuick
import com.yap.native

// A key cap (ui/kbd.tsx): Martian 11/600, 2 px corners. `cap` is a tint-2
// key with a 2 px ink-2 bottom edge, 24 high; `outline` a line-strong
// outline in ink-2.
Rectangle {
    id: root

    property string text
    property bool outline: false

    implicitWidth: Math.max(24, label.implicitWidth + 12)
    implicitHeight: 24
    radius: Theme.radius
    color: outline ? "transparent" : Theme.tint2
    border.width: outline ? 1 : 0
    border.color: Theme.lineStrong
    Accessible.ignored: true

    Rectangle {
        visible: !root.outline
        anchors.bottom: parent.bottom
        width: parent.width
        height: 2
        color: Theme.ink2
    }
    Text {
        id: label
        anchors.centerIn: parent
        anchors.verticalCenterOffset: root.outline ? 0 : -1
        textFormat: Text.PlainText
        text: root.text
        color: root.outline ? Theme.ink2 : Theme.ink1
        font: Theme.type.kbd
    }
}
