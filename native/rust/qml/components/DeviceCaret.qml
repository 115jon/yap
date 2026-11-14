import QtQuick
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// A 10 x 40 caret beside a dock key; it opens (and closes) its device menu.
T.AbstractButton {
    id: caret

    required property string label
    // Its device menu, made as it opens (a LazyPopup of a DeviceMenu).
    required property LazyPopup menu
    readonly property bool menuOpen: menu.item?.visible ?? false
    // Over a nameplate (the account row): the plate's ink, lighter at rest.
    property color plateInk: "transparent"
    property color plateMuted: "transparent"

    implicitWidth: 10
    implicitHeight: 40
    focusPolicy: Qt.StrongFocus
    Accessible.role: Accessible.ButtonMenu
    Accessible.name: label
    onClicked: caret.menuOpen ? caret.menu.item.close() : caret.menu.load()?.openAbove(caret)

    contentItem: Item {
        Icon {
            anchors.centerIn: parent
            name: "chevronDown"
            size: 10
            color: caret.plateInk.a > 0 ? (caret.hovered || caret.menuOpen ? caret.plateInk : caret.plateMuted)
                : caret.hovered || caret.menuOpen ? Theme.ink1 : Theme.ink3
        }
    }
    background: Rectangle {
        radius: Theme.radius
        color: "transparent"
        border.width: 2
        border.color: Theme.focus
        visible: caret.visualFocus
    }

    HoverHandler { cursorShape: Qt.PointingHandCursor }
}
