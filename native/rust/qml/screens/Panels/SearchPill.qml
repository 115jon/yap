import QtQuick
import com.yap.native
import Yap.Ui

// What a result carries (SearchResultItem.tsx ATTACHMENT_PILL): 24 high,
// up to 200, ply-2 on a hairline, a 12 px glyph and the name at 12.
Rectangle {
    id: root

    property string icon
    property string label
    property string spoken: label
    property bool muted: false

    implicitWidth: Math.min(200, row.implicitWidth + 12)
    implicitHeight: Theme.keyXs
    radius: Theme.radius
    color: Theme.ply2
    border.width: 1
    border.color: Theme.line
    Accessible.role: Accessible.StaticText
    Accessible.name: spoken

    Row {
        id: row
        x: 6
        anchors.verticalCenter: parent.verticalCenter
        spacing: 6
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.icon.length > 0
            name: root.icon
            size: Theme.iconXs
            color: Theme.ink3
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, 200 - 12 - (root.icon.length > 0 ? Theme.iconXs + 6 : 0))
            textFormat: Text.PlainText
            text: root.label
            elide: Text.ElideRight
            color: root.muted ? Theme.ink3 : Theme.ink2
            font: Theme.type.caption
        }
    }
}
