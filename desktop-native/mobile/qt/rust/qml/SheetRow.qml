import QtQuick
import Yap.Ui

// An action row in a sheet (DESIGN.md Menu, on touch): 48 high, a 20 px
// glyph in the icon column, Geist 15 at 500. `danger` rows are danger ink.
// The press tints the row; nothing hovers.
Item {
    id: row

    required property string text
    property string iconName: ""
    property bool danger: false
    // A trailing hint (a count), in the number face.
    property string hint: ""

    signal triggered()

    width: parent ? parent.width : 0
    height: Math.max(Theme.controlLg, label.implicitHeight + 16)

    Accessible.role: Accessible.MenuItem
    Accessible.name: text
    Accessible.onPressAction: row.triggered()

    Rectangle {
        anchors.fill: parent
        color: tap.pressed ? Theme.tint2 : "transparent"
        EaseColor on color {}
    }
    Icon {
        visible: row.iconName.length > 0
        x: 16
        anchors.verticalCenter: parent.verticalCenter
        name: row.iconName
        size: Theme.iconLg
        color: row.danger ? Theme.dangerInk : Theme.ink2
    }
    Text {
        id: label
        anchors.left: parent.left
        anchors.leftMargin: 16 + Theme.iconLg + 16
        anchors.right: hintText.visible ? hintText.left : parent.right
        anchors.rightMargin: 16
        anchors.verticalCenter: parent.verticalCenter
        text: row.text
        textFormat: Text.PlainText
        elide: Text.ElideRight
        color: row.danger ? Theme.dangerInk : Theme.ink1
        font: Theme.type.setting
    }
    Text {
        id: hintText
        visible: row.hint.length > 0
        anchors.right: parent.right
        anchors.rightMargin: 16
        anchors.verticalCenter: parent.verticalCenter
        text: row.hint
        textFormat: Text.PlainText
        color: Theme.ink3
        font: Theme.type.num
    }
    TapHandler {
        id: tap
        onTapped: row.triggered()
    }
}
