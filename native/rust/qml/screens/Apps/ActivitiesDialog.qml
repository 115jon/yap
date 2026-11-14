pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// Activities (VoiceAppsModal.tsx): one card per activity; the card starts
// it for the call. Warp Rush is not built here yet.
AppPopup {
    id: root

    property int people: 1

    title: qsTr("Activities")
    width: 420
    maxHeight: 300

    Column {
        anchors.fill: parent
        anchors.margins: 12
        anchors.topMargin: 0
        spacing: 12
        Text {
            width: parent.width
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            text: root.people > 1 ? qsTr("Starts for all %1 people in this call.").arg(root.people)
                : qsTr("Starts for everyone in this call.")
            color: Theme.ink2
            font: Theme.type.description
        }
        Row {
            width: parent.width
            spacing: 8
            Card {
                title: qsTr("Daily Word")
                body: qsTr("One shared puzzle a day. Everyone in the call sees how the others are doing.")
                onClicked: {
                    Apps.startActivity("wordle")
                    root.close()
                }
            }
            Card {
                enabled: false
                title: qsTr("Warp Rush")
                body: qsTr("Coming later in this app.")
            }
        }
    }

    component Card: T.Button {
        id: card
        property string title
        property string body
        width: (root.width - 24 - 8) / 2
        height: 168
        padding: 12
        opacity: enabled ? 1 : 0.5
        Accessible.role: Accessible.Button
        Accessible.name: title
        background: Rectangle {
            radius: Theme.radius
            color: Theme.ply2
            border.width: card.visualFocus ? 2 : 1
            border.color: card.visualFocus ? Theme.focus : Theme.line
            Rectangle {
                anchors.fill: parent
                radius: parent.radius
                color: Theme.ink1
                opacity: card.down ? 0.1 : card.hovered ? 0.06 : 0
            }
        }
        contentItem: Column {
            spacing: 8
            Rectangle {
                width: parent.width
                height: 64
                radius: Theme.radius
                color: Theme.ply1
                border.width: 1
                border.color: Theme.line
                Grid {
                    anchors.centerIn: parent
                    columns: 5
                    spacing: 2
                    Repeater {
                        model: card.enabled ? ["a", "p", "a", "a", "a", "c", "c", "a", "p", "a", "c", "c", "c", "c", "c"] : []
                        delegate: Rectangle {
                            required property string modelData
                            width: 12
                            height: 12
                            radius: 2
                            color: modelData === "c" ? Theme.talk : modelData === "p" ? Theme.idle : Theme.tint3
                        }
                    }
                }
                Icon {
                    visible: !card.enabled
                    anchors.centerIn: parent
                    name: "gamepad"
                    size: Theme.iconLg
                    color: Theme.ink3
                }
            }
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: card.title
                color: Theme.ink1
                font: Theme.type.row
            }
            Text {
                width: parent.width
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                text: card.body
                color: Theme.ink2
                font: Theme.type.caption
            }
        }
        HoverHandler { cursorShape: Qt.PointingHandCursor }
    }
}
