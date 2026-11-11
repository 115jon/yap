import QtQuick
import com.yap.native
import Yap.Ui

// A match result (Deadlock.tsx Result / WL): a win is a solid ink-1 key,
// a loss an outlined one. `long` prints "Win"/"Loss" at 20 high; otherwise
// "W"/"L" at 28 x 14 (the caller may widen or heighten it).
Rectangle {
    id: chip

    property bool win: false
    property bool full: false

    implicitWidth: full ? word.implicitWidth + 12 : 28
    implicitHeight: full ? 20 : 14
    width: implicitWidth
    height: implicitHeight
    radius: full ? Theme.radius : 1
    color: win ? Theme.ink1 : "transparent"
    border.width: win ? 0 : 1
    border.color: Theme.lineStrong
    Accessible.role: Accessible.StaticText
    Accessible.name: win ? qsTr("Win") : qsTr("Loss")

    Text {
        id: word
        anchors.centerIn: parent
        textFormat: Text.PlainText
        text: chip.full ? (chip.win ? qsTr("Win") : qsTr("Loss")) : (chip.win ? "W" : "L")
        color: chip.win ? Theme.ply0 : chip.full ? Theme.ink2 : Theme.ink3
        font: Qt.font({ family: "Yap Martian Num SemiBold", pixelSize: 11, weight: Font.DemiBold,
                        letterSpacing: chip.full ? 0.88 : 0, capitalization: Font.AllUppercase })
    }
}
