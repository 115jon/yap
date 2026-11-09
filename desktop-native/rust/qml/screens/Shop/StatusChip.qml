import QtQuick
import com.yap.native

// "Leaves in 3 days" / "Retired" on collection art (ShopView.tsx
// StatusChip): its own media scrim and white ink, a clock or a lock.
Rectangle {
    id: root

    property string label
    property string tone: "leaving"

    width: chipRow.implicitWidth + 12
    height: 20
    radius: Theme.radius
    color: Qt.rgba(0, 0, 0, 0.64)
    Accessible.role: Accessible.StaticText
    Accessible.name: label

    Row {
        id: chipRow
        anchors.centerIn: parent
        spacing: 4
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            name: root.tone === "leaving" ? "clock" : "lock"
            size: Theme.iconXs
            color: "#ffffff"
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: root.label
            color: "#ffffff"
            font: Theme.type.captionStrong
        }
    }
}
