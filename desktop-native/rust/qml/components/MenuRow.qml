import QtQuick
import QtQuick.Templates as T
import com.yap.native

// A menu row (ui/menu.ts): 36 high, px-3, gap 12, Geist 14/500, an
// optional 16 px ink-2 glyph; highlight tint-2 with no transition. `danger`
// rows use danger-ink and highlight on danger-tint.
T.MenuItem {
    id: control

    property string glyph: ""
    property bool danger: false

    implicitWidth: Math.max(200, implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: 36
    leftPadding: 12
    rightPadding: 12

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
