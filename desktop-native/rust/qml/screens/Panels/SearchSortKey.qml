import QtQuick
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// A sort key in the results toolbar: 24 high, px-2, 12/500, ink-3 that
// turns ink-1 on hover; pressed is tint-2 and ink-1.
T.Button {
    id: control

    checkable: false
    implicitWidth: label.implicitWidth + 16
    implicitHeight: Theme.keyXs
    padding: 0
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
