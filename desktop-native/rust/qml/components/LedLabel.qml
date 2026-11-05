import QtQuick
import com.yap.native

// An LED and its silkscreen word (ui/led.tsx LedLabel): `ledState` live, idle,
// talk or off. `plain` keeps the word in ink-2 instead of the LED's ink.
Row {
    id: root

    property string text
    property string ledState: "off"
    property bool plain: false

    readonly property color led: ledState === "live" ? Theme.live
        : ledState === "idle" ? Theme.idle
        : ledState === "talk" ? Theme.talk
        : Theme.lineStrong

    spacing: 8
    height: 20
    Accessible.role: Accessible.StaticText
    Accessible.name: text

    Led {
        anchors.verticalCenter: parent.verticalCenter
        lit: root.ledState !== "off"
        color: root.led
    }
    Text {
        anchors.verticalCenter: parent.verticalCenter
        textFormat: Text.PlainText
        text: root.text
        color: root.plain || root.ledState === "off" ? Theme.ink2
            : root.ledState === "live" ? Theme.liveInk
            : root.ledState === "idle" ? Theme.warningInk
            : Theme.successInk
        font: Theme.type.silk
    }
}
