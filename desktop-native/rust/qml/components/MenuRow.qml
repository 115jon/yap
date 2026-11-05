import QtQuick
import QtQuick.Templates as T
import com.yap.native

// A menu row (ui/menu.ts): 36 high, px-3, gap 12, Geist 14/500, an
// optional 16 px ink-2 glyph; highlight tint-2 with no transition. `danger`
// rows use danger-ink and highlight on danger-tint. A `switchable` row is a
// setting: its LED at the right end is lit while `on`.
T.MenuItem {
    id: control

    property string glyph: ""
    property bool danger: false
    property bool switchable: false
    property bool on: false

    implicitWidth: Math.max(200, implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: 36
    leftPadding: 12
    rightPadding: switchable ? 36 : 12

    Accessible.role: switchable ? Accessible.CheckBox : Accessible.MenuItem
    Accessible.checkable: switchable
    Accessible.checked: on

    indicator: Led {
        x: control.width - width - 14
        y: (control.height - height) / 2
        visible: control.switchable
        size: 8
        lit: control.on
    }

    contentItem: Row {
        spacing: 12
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            visible: control.glyph.length > 0
            name: control.glyph
            size: Theme.iconSm
            color: control.danger ? Theme.dangerInk : Theme.ink2
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: control.text
            color: control.danger ? Theme.dangerInk : control.enabled ? Theme.ink1 : Theme.ink3
            font: Theme.type.row
        }
    }
    background: Rectangle {
        radius: Theme.radius
        color: control.highlighted || control.down ? (control.danger ? Theme.dangerTint : Theme.tint2) : "transparent"
    }
}
