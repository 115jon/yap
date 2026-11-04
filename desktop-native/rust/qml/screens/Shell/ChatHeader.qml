import QtQuick
import com.yap.native

// The text channel header (ChatHeader.tsx): 48, px-4. The title key (the
// channel glyph at 20 in ink-3, Bricolage 18/800) opens the channel's
// details; on the right pins, notifications and members (40 px muted keys
// behind a seam), then the underline search field with its "/" key. The
// panels these open come later, so the keys only take hover for now.
Item {
    id: root

    implicitHeight: Theme.header

    Rectangle {
        id: title
        x: 16
        anchors.verticalCenter: parent.verticalCenter
        height: 32
        width: titleRow.implicitWidth + 16
        radius: Theme.radius
        color: titleHover.hovered ? Theme.tint1 : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.dur1 } }
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Channel details")
        Row {
            id: titleRow
            x: 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                visible: Nav.activeChannelId.length > 0
                name: "textChannel"
                size: Theme.iconLg
                color: titleHover.hovered ? Theme.ink2 : Theme.ink3
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: Nav.activeChannelName
                color: Theme.ink1
                font: Theme.type.columnTitle
            }
        }
        HoverHandler { id: titleHover }
    }

    Text {
        anchors.left: title.right
        anchors.leftMargin: 12
        anchors.right: actions.left
        anchors.rightMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        visible: Session.status.length > 0
        text: Session.status
        elide: Text.ElideRight
        color: Theme.ink3
        font: Theme.type.caption
    }

    Row {
        id: actions
        anchors.right: parent.right
        anchors.rightMargin: 16
        anchors.verticalCenter: parent.verticalCenter
        spacing: 8

        Row {
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            rightPadding: 8
            IconKey { iconName: "pin"; tone: "muted"; tipSide: "bottom"; tip: qsTr("Pinned messages") }
            IconKey { iconName: "bell"; tone: "muted"; tipSide: "bottom"; tip: qsTr("Notifications") }
            IconKey { iconName: "members"; tone: "muted"; tipSide: "bottom"; tip: qsTr("Show members") }
            Rectangle {
                width: 1
                height: 40
                color: Theme.line
            }
        }

        // C1 underline field: no box, a 2 px line-field rule, "/" key.
        Item {
            anchors.verticalCenter: parent.verticalCenter
            width: 224
            height: 38
            visible: root.width >= 720
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Search messages")
            Row {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width
                spacing: 8
                Icon {
                    anchors.verticalCenter: parent.verticalCenter
                    name: "search"
                    size: Theme.iconSm
                    color: searchHover.hovered ? Theme.ink2 : Theme.ink3
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - 16 - 8 - 8 - slash.width
                    text: qsTr("Search")
                    color: searchHover.hovered ? Theme.ink2 : Theme.ink3
                    font: Theme.type.description
                }
                Rectangle {
                    id: slash
                    anchors.verticalCenter: parent.verticalCenter
                    width: 20
                    height: 20
                    radius: Theme.radius
                    color: "transparent"
                    border.width: 1
                    border.color: Theme.lineStrong
                    Text {
                        anchors.centerIn: parent
                        text: "/"
                        color: Theme.ink3
                        font: Theme.type.kbd
                    }
                }
            }
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 2
                color: searchHover.hovered ? Theme.ink2 : Theme.lineField
                Behavior on color { ColorAnimation { duration: Theme.dur1 } }
            }
            HoverHandler { id: searchHover }
        }
    }

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.line
    }
}
