import QtQuick
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// A tab in a voice app's panel (ui/tabs.tsx): ink-1 when chosen, its count
// beside it when there is one. The 2 px rule is the strip's TabRule, which
// slides between tabs.
T.AbstractButton {
    id: tab

    property bool on: false
    property int count: -1

    implicitWidth: implicitContentWidth + leftPadding + rightPadding
    implicitHeight: 32
    leftPadding: 8
    rightPadding: 8
    focusPolicy: Qt.StrongFocus
    Accessible.role: Accessible.PageTab
    Accessible.name: text
    Accessible.selected: on

    contentItem: Row {
        spacing: 6
        Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: tab.text
            color: tab.on || tab.hovered ? Theme.ink1 : Theme.ink2
            EaseColor on color {}
            font: Theme.type.controlSm
        }
        Text {
            visible: tab.count >= 0
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: tab.count
            color: Theme.ink3
            font: Theme.type.numSm
        }
    }
    background: Item {
        Rectangle {
            visible: tab.visualFocus
            anchors.fill: parent
            color: "transparent"
            radius: Theme.radius
            border.width: 2
            border.color: Theme.focus
        }
    }

    HoverHandler { cursorShape: Qt.PointingHandCursor }
}
