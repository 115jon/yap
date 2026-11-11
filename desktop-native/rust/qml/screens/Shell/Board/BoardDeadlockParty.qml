pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// The owner's party (Deadlock.tsx PartyStack / PartyLine): up to three
// 16 px faces ringed in the plate's ply, then "With Mo and Ash" (or the
// caller's `text`). Hidden with no party.
Item {
    id: line

    required property var dl
    property var party: dl.party
    // Overrides the "With …" words.
    property string text: ""
    property color ink: Theme.ink2

    visible: party.length > 0
    implicitHeight: 20
    height: visible ? 20 : 0
    Accessible.role: Accessible.StaticText
    Accessible.name: words.text

    Row {
        id: stack
        anchors.verticalCenter: parent.verticalCenter
        x: -2
        spacing: -6
        Repeater {
            model: line.party.slice(0, 3)
            delegate: Rectangle {
                id: ring
                required property var modelData
                width: 20
                height: 20
                radius: Theme.radius + 1
                color: Theme.ply2
                Avatar {
                    anchors.centerIn: parent
                    size: 16
                    initials: String(ring.modelData.name ?? "?").charAt(0).toUpperCase()
                    source: ring.modelData.avatar ?? ""
                }
            }
        }
    }
    Text {
        id: words
        x: stack.x + stack.implicitWidth + 6
        width: Math.max(0, line.width - x)
        anchors.verticalCenter: parent.verticalCenter
        textFormat: Text.PlainText
        text: line.text.length > 0 ? line.text
            : qsTr("With %1").arg(line.dl.joinNames(line.party.map(m => m.name)))
        elide: Text.ElideRight
        color: line.ink
        font: Theme.type.caption
    }
}
