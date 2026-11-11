import QtQuick
import com.yap.native
import Yap.Ui

// One suggestion or idle row (SuggestionContent.tsx in OPTION_ROW): 32
// high, px-2, a 20 px lead (glyph or picture), the label at 13/500 (a
// key's hint after it in ink-3; a person's name in their role colour), and
// a trailing meta at 12 in ink-3. Tint-2 while lit, tint-1 on hover.
Rectangle {
    id: root

    // {id, kind, label, hint, meta, icon, avatar, initials, color}
    required property var option
    property bool lit: false

    signal picked()
    signal hovered()

    implicitHeight: Theme.keySm
    radius: Theme.radius
    color: lit || activeFocus ? Theme.tint2 : hover.hovered ? Theme.tint1 : "transparent"
    EaseColor on color {}
    Accessible.role: Accessible.ListItem
    Accessible.name: root.option.label + (root.option.hint ? " " + root.option.hint : "")
        + (root.option.meta ? ", " + root.option.meta : "")
    Accessible.selected: lit
    Accessible.onPressAction: root.picked()
    Keys.onReturnPressed: root.picked()
    Keys.onSpacePressed: root.picked()

    HoverHandler {
        id: hover
        cursorShape: Qt.PointingHandCursor
        onPointChanged: if (!root.lit) root.hovered()
    }
    TapHandler { onTapped: root.picked() }

    Item {
        id: lead
        x: 8
        anchors.verticalCenter: parent.verticalCenter
        width: 20
        height: 20
        Avatar {
            anchors.centerIn: parent
            visible: root.option.kind === "user"
            size: 20
            initials: root.option.initials ?? ""
            source: root.option.avatar ?? ""
        }
        Icon {
            anchors.centerIn: parent
            visible: root.option.kind !== "user"
            name: root.option.icon ?? ""
            size: Theme.iconSm
            color: Theme.ink3
        }
    }

    Text {
        id: label
        anchors.left: lead.right
        anchors.leftMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        width: Math.min(implicitWidth, root.width - x - 8 - (meta.visible ? Math.min(meta.implicitWidth, root.width / 2) + 12 : 0))
        textFormat: Text.PlainText
        text: root.option.label
        elide: Text.ElideRight
        color: root.option.kind === "recent" ? Theme.ink2
            : (root.option.color ?? "").length > 0 ? root.option.color : Theme.ink1
        font: root.option.kind === "recent" ? Theme.type.description : Theme.type.occupant
    }
    Text {
        anchors.left: label.right
        anchors.leftMargin: 6
        anchors.verticalCenter: parent.verticalCenter
        anchors.right: parent.right
        anchors.rightMargin: 8
        visible: (root.option.hint ?? "").length > 0
        textFormat: Text.PlainText
        text: root.option.hint ?? ""
        elide: Text.ElideRight
        color: Theme.ink3
        font: Theme.type.description
    }
    Text {
        id: meta
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        width: Math.min(implicitWidth, root.width / 2)
        visible: (root.option.meta ?? "").length > 0
        horizontalAlignment: Text.AlignRight
        textFormat: Text.PlainText
        text: root.option.meta ?? ""
        elide: Text.ElideRight
        color: Theme.ink3
        font: Theme.type.caption
    }
}
