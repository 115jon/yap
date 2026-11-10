import QtQuick
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// A menu separator (ui/menu.ts MENU_SEPARATOR): a 1 px line rule across
// the panel's padding, 4 px above and below.
T.MenuSeparator {
    implicitWidth: 200
    implicitHeight: 9
    leftInset: -4
    rightInset: -4
    background: Item {
        Rectangle {
            width: parent.width
            height: 1
            anchors.verticalCenter: parent.verticalCenter
            color: Theme.line
        }
    }
}
