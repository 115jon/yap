import QtQuick
import Yap.Ui

// A row of the account menu (ui/menu.ts MENU_ITEM; the desktop's account
// popover, the phone's account panel): a 16 px glyph (or an LED) column,
// Geist 14/500, an optional hint in Martian (the theme's name), an optional
// trailing glyph. 36 px under a pointer, 44 under a finger; tint-2 while
// hovered or pressed, tint-1 while selected.
Rectangle {
    id: key

    property string glyph: ""
    // An LED in the glyph column instead (a presence).
    property string led: ""
    property string text
    property string hint: ""
    property string trailingGlyph: ""
    property bool danger: false
    property bool quiet: false
    property bool selected: false
    signal triggered()

    implicitWidth: 240
    implicitHeight: Math.max(Theme.touch ? 44 : 36, label.implicitHeight + 12)
    radius: Theme.radius
    color: hover.hovered || tap.pressed ? (danger ? Theme.dangerTint : Theme.tint2)
        : selected ? Theme.tint1 : "transparent"
    EaseColor on color {}
    Accessible.role: Accessible.MenuItem
    Accessible.name: hint.length > 0 ? "%1, %2".arg(text).arg(hint) : text
    Accessible.onPressAction: key.triggered()

    Item {
        id: lead
        x: 12
        width: 16
        height: parent.height
        Icon {
            anchors.centerIn: parent
            visible: key.glyph.length > 0 && key.led.length === 0
            name: key.glyph
            size: Theme.iconSm
            color: key.danger ? Theme.dangerInk : Theme.ink2
        }
        Led {
            anchors.centerIn: parent
            visible: key.led.length > 0
            size: 10
            lit: key.led !== "offline"
            color: Theme.presenceLed(key.led)
        }
    }
    // The label keeps its width; the hint takes what is left (at least 48).
    readonly property real end: trail.visible ? trail.x - 8 : width - 12
    Text {
        id: label
        anchors.left: lead.right
        anchors.leftMargin: 12
        width: Math.min(implicitWidth, key.end - x - (hintText.visible ? 56 : 0))
        anchors.verticalCenter: parent.verticalCenter
        textFormat: Text.PlainText
        text: key.text
        elide: Text.ElideRight
        color: key.danger ? Theme.dangerInk : key.quiet ? Theme.ink2 : Theme.ink1
        font: Theme.type.row
    }
    Text {
        id: hintText
        anchors.left: label.right
        anchors.leftMargin: 8
        anchors.right: trail.visible ? trail.left : parent.right
        anchors.rightMargin: trail.visible ? 8 : 12
        anchors.verticalCenter: parent.verticalCenter
        visible: key.hint.length > 0
        horizontalAlignment: Text.AlignRight
        textFormat: Text.PlainText
        text: key.hint
        elide: Text.ElideRight
        color: Theme.ink3
        font: Theme.type.numSm
    }
    Icon {
        id: trail
        anchors.right: parent.right
        anchors.rightMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        visible: key.trailingGlyph.length > 0
        name: key.trailingGlyph
        size: key.trailingGlyph === "check" ? Theme.iconSm : Theme.iconXs
        color: key.trailingGlyph === "check" ? Theme.ink1 : Theme.ink3
    }
    HoverHandler {
        id: hover
        enabled: Theme.hoverUi
        cursorShape: Qt.PointingHandCursor
    }
    TapHandler {
        id: tap
        onTapped: key.triggered()
    }
}
