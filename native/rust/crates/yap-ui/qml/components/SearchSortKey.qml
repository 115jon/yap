import QtQuick
import QtQuick.Templates as T
import Yap.Ui

// A sort key in the results toolbar: 24 high, px-2, 12/500, ink-3 that
// turns ink-1 on hover; pressed is tint-2 and ink-1. On touch it is drawn
// at 32 and answers Theme.hitMin high (its neighbours sit beside it).
T.Button {
    id: control

    checkable: false
    implicitWidth: label.implicitWidth + 16
    implicitHeight: Theme.touch ? Theme.keySm : Theme.keyXs
    padding: 0
    containmentMask: Theme.hitMin > Math.min(width, height) ? hit : null
    Item {
        id: hit
        anchors.centerIn: parent
        width: control.width
        height: Math.max(control.height, Theme.hitMin)
    }
    Accessible.role: Accessible.Button
    Accessible.name: text
    Accessible.checkable: true
    Accessible.checked: checked

    contentItem: Text {
        id: label
        textFormat: Text.PlainText
        text: control.text
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        color: control.checked || control.hovered ? Theme.ink1 : Theme.ink3
        font: Theme.type.captionStrong
        Behavior on color { ColorAnimation { duration: Theme.dur1 } }
    }
    background: Rectangle {
        radius: Theme.radius
        color: control.checked ? Theme.tint2 : "transparent"
        border.width: control.visualFocus ? 2 : 0
        border.color: Theme.focus
    }

    HoverHandler { cursorShape: Qt.PointingHandCursor }
}
