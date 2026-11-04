import QtQuick
import com.yap.native

// A small call key: glyph plus silkscreen word on tint-1; lit keys take the
// state's tint and ink.
Rectangle {
    id: key

    required property string label
    required property string icon
    property string iconVariant: "idle"
    property bool lit: false
    property color litColor: Theme.tint3
    property color litInk: Theme.ink1
    signal clicked()

    readonly property color ink: lit ? litInk : keyHover.hovered ? Theme.ink1 : Theme.ink2

    height: 28
    width: content.implicitWidth + 20
    radius: Theme.radius
    color: lit ? litColor : keyTap.pressed ? Theme.tint3 : keyHover.hovered ? Theme.tint2 : Theme.tint1
    Behavior on color { ColorAnimation { duration: Theme.dur1 } }

    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: label
    Accessible.onPressAction: key.clicked()
    Keys.onSpacePressed: key.clicked()

    Row {
        id: content
        anchors.centerIn: parent
        spacing: 6
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            name: key.icon
            variant: key.iconVariant
            size: Theme.iconSm
            color: key.ink
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: key.label
            color: key.ink
            font: Theme.type.silk
        }
    }
    Rectangle {
        anchors.fill: parent
        anchors.margins: -2
        visible: key.activeFocus
        color: "transparent"
        radius: Theme.radius
        border.width: 2
        border.color: Theme.focus
    }
    HoverHandler { id: keyHover; cursorShape: Qt.PointingHandCursor }
    TapHandler { id: keyTap; onTapped: key.clicked() }
}
