import QtQuick
import com.yap.native

// UnreadBanner.tsx: pinned over the top of the list (it overlays, so the
// column never jumps): the "New" label, "N messages since [day at] time",
// a badge seam, and Mark as read.
Rectangle {
    id: root

    required property int count
    required property string sinceTime
    required property string sinceDate
    required property int sinceDay
    required property int today
    signal markRead()

    readonly property string dayPart: sinceDay === today ? ""
        : sinceDay === today - 1 ? qsTr("yesterday at ") : qsTr("%1 at ").arg(sinceDate)

    height: 32
    color: Theme.ply0
    Accessible.role: Accessible.AlertMessage
    Accessible.name: summary.text

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.line
    }

    Rectangle {
        id: rule
        x: 16
        anchors.verticalCenter: parent.verticalCenter
        width: 24
        height: 2
        color: Theme.badge
    }
    Text {
        id: label
        anchors.left: rule.right
        anchors.leftMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        text: qsTr("New")
        color: Theme.dangerInk
        font: Theme.type.silk
    }
    Text {
        id: summary
        anchors.left: label.right
        anchors.leftMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        width: Math.min(implicitWidth, markKey.x - x - 28)
        elide: Text.ElideRight
        textFormat: Text.StyledText
        text: "<font color=\"%1\">%2</font> ".arg(Theme.ink1).arg(root.count)
            + (root.count === 1 ? qsTr("message since %1%2") : qsTr("messages since %1%2"))
                .arg(root.dayPart).arg(root.sinceTime)
        color: Theme.ink2
        font: Theme.type.caption
    }
    Rectangle {
        anchors.left: summary.right
        anchors.leftMargin: 12
        anchors.right: markKey.left
        anchors.rightMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        height: 1
        color: Theme.badge
    }
    // Ghost key: "Mark as read" with a check.
    Rectangle {
        id: markKey
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        width: markRow.implicitWidth + 16
        height: 24
        radius: Theme.radius
        color: markHover.hovered ? Theme.tint1 : "transparent"
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Mark as read")
        Row {
            id: markRow
            anchors.centerIn: parent
            spacing: 6
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Mark as read")
                color: markHover.hovered ? Theme.ink1 : Theme.ink2
                font: Theme.type.captionStrong
            }
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: "check"
                size: Theme.iconSm
                color: markHover.hovered ? Theme.ink1 : Theme.ink2
            }
        }
        HoverHandler { id: markHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: root.markRead() }
    }
}
