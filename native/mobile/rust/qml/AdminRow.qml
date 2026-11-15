import QtQuick
import Yap.Mobile
import Yap.Ui

// A row in Server settings: an optional leading item (a glyph, a role's
// dot, an avatar), the label over an optional hint, then a chevron when it
// opens a page (or `trailing` keys). 48 high at least; the whole row is the
// key. `danger` rows are danger ink.
Item {
    id: row

    property string text
    property string hint: ""
    property string iconName: ""
    property bool chevron: true
    property bool danger: false
    // Drawn in the leading slot instead of the glyph (24 wide at most).
    property Component leading: null
    // Keys at the trailing end, in place of the chevron.
    default property alias trailing: trailingRow.data

    signal opened()

    width: parent ? parent.width : 0
    height: Math.max(Theme.controlLg, labels.implicitHeight + 16)
    opacity: enabled ? 1 : 0.45
    Accessible.role: Accessible.Button
    Accessible.name: text
    Accessible.description: hint
    Accessible.onPressAction: if (row.enabled) row.opened()

    Rectangle {
        anchors.fill: parent
        anchors.leftMargin: -8
        anchors.rightMargin: -8
        radius: Theme.radius
        color: tap.pressed && row.enabled ? Theme.tint2 : "transparent"
        EaseColor on color {}
    }
    Item {
        id: lead
        visible: row.iconName.length > 0 || row.leading !== null
        width: visible ? 24 : 0
        height: 24
        anchors.verticalCenter: parent.verticalCenter
        Icon {
            visible: row.leading === null
            anchors.centerIn: parent
            name: row.iconName
            size: Theme.iconLg
            color: row.danger ? Theme.dangerInk : Theme.ink2
        }
        Loader {
            anchors.centerIn: parent
            sourceComponent: row.leading
        }
    }
    Column {
        id: labels
        anchors.left: lead.right
        anchors.leftMargin: lead.visible ? 16 : 0
        anchors.right: trailingRow.children.length > 0 ? trailingRow.left : chevronIcon.visible ? chevronIcon.left : parent.right
        anchors.rightMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        spacing: 2
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: row.text
            elide: Text.ElideRight
            color: row.danger ? Theme.dangerInk : Theme.ink1
            font: Theme.type.setting
        }
        Text {
            width: parent.width
            visible: row.hint.length > 0
            textFormat: Text.PlainText
            text: row.hint
            wrapMode: Text.Wrap
            maximumLineCount: 3
            elide: Text.ElideRight
            color: Theme.ink3
            font: Theme.type.caption
        }
    }
    Icon {
        id: chevronIcon
        visible: row.chevron && trailingRow.children.length === 0
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        name: "chevronRight"
        size: Theme.iconMd
        color: Theme.ink3
    }
    Row {
        id: trailingRow
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: 4
    }
    TapHandler {
        id: tap
        enabled: row.enabled
        onTapped: {
            Native.haptic(Native.Tick)
            row.opened()
        }
    }
}
