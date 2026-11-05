import QtQuick
import QtQuick.Templates as T
import com.yap.native

// A selectable list row (RoleList, the override list, pickers): 32 high
// (`tall`: 40), tint-2 while current, tint-1 on hover, the focus ring on
// keyboard focus. Its content is laid out by the user.
T.AbstractButton {
    id: row

    property bool current: false
    property bool tall: false

    implicitHeight: tall ? 40 : 32
    focusPolicy: Qt.StrongFocus
    Accessible.role: Accessible.Button
    Accessible.selected: current

    background: Rectangle {
        radius: Theme.radius
        color: row.current ? Theme.tint2 : row.hovered ? Theme.tint1 : "transparent"
        border.width: row.visualFocus ? 2 : 0
        border.color: Theme.focus
        Behavior on color { ColorAnimation { duration: Theme.dur1 } }
    }
}
