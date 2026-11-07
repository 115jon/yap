import QtQuick
import QtQuick.Templates as T
import com.yap.native

// A menu row (ui/menu.ts): 36 high, px-3, gap 12, Geist 14/500, an
// optional 16 px ink-2 glyph; highlight tint-2 with no transition. `danger`
// rows use danger-ink and highlight on danger-tint. A `switchable` row is a
// setting: its LED at the right end is lit while `on`. A row that opens a
// submenu (a MenuPanel child) takes the submenu's glyph and ends in a
// chevron (dropdown-menu.tsx SubmenuTrigger). `image` puts a 16 px picture
// in the glyph's place (an emoji), with `imageText` drawn while it loads.
T.MenuItem {
    id: control

    property string glyph: subMenu ? (subMenu.glyph || "") : ""
    property url image: ""
    property string imageText: ""
    property bool danger: false
    property bool switchable: false
    property bool on: false
    // The menu's lead action (Watch stream): semibold.
    property bool bold: false
    // A second line under the label (12, ink-3), as StreamMenuEntries
    // draws an entry's description.
    property string description: ""

    implicitWidth: Math.max(200, implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: description.length > 0 ? 36 + 16 + 6 : 36
    visible: subMenu ? subMenu.shown !== false : true
    height: visible ? implicitHeight : 0
    leftPadding: 12
    rightPadding: switchable || subMenu ? 36 : 12

    Accessible.role: switchable ? Accessible.CheckBox : Accessible.MenuItem
    Accessible.checkable: switchable
    Accessible.checked: on

    arrow: Icon {
        x: control.width - width - 12
        y: (control.height - height) / 2
        visible: control.subMenu !== null
        name: "chevronRight"
        size: Theme.iconSm
        color: Theme.ink3
    }

    indicator: Led {
        x: control.width - width - 14
        y: (control.height - height) / 2
        visible: control.switchable
        size: 8
        lit: control.on
    }

    contentItem: Row {
        spacing: 12
        Item {
            anchors.verticalCenter: parent.verticalCenter
            visible: control.image.toString().length > 0 || control.imageText.length > 0
            width: Theme.iconSm
            height: Theme.iconSm
            Image {
                id: rowImage
                anchors.fill: parent
                source: control.image
                sourceSize: Qt.size(32, 32)
                asynchronous: true
            }
            // The emoji itself until (or unless) its picture loads.
            Text {
                anchors.centerIn: parent
                visible: rowImage.status !== Image.Ready
                textFormat: Text.PlainText
                text: control.imageText
                font.family: "Segoe UI Emoji"
                font.pixelSize: 14
            }
        }
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            visible: control.glyph.length > 0
            name: control.glyph
            size: Theme.iconSm
            color: control.danger ? Theme.dangerInk : Theme.ink2
        }
        Column {
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Text {
                textFormat: Text.PlainText
                text: control.text
                color: control.danger ? Theme.dangerInk : control.enabled ? Theme.ink1 : Theme.ink3
                font: control.bold ? Theme.type.panelTitle : Theme.type.row
            }
            Text {
                visible: control.description.length > 0
                width: Math.max(0, control.availableWidth - (control.glyph.length > 0 ? 28 : 0))
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: control.description
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
    }
    background: Rectangle {
        radius: Theme.radius
        color: control.highlighted || control.down ? (control.danger ? Theme.dangerTint : Theme.tint2) : "transparent"
    }
}
