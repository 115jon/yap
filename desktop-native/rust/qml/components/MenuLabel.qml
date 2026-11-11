import QtQuick
import com.yap.native
import Yap.Ui

// A menu's title or group label (ui/menu.ts MENU_LABEL): a silkscreen row
// in ink-3, 32 high, px-3.
Item {
    id: root

    property alias text: label.text

    implicitWidth: label.implicitWidth + 24
    implicitHeight: 32
    Accessible.role: Accessible.StaticText
    Accessible.name: text

    Text {
        id: label
        textFormat: Text.PlainText
        x: 12
        width: parent.width - 24
        anchors.verticalCenter: parent.verticalCenter
        elide: Text.ElideRight
        color: Theme.ink3
        font: Theme.type.silk
    }
}
