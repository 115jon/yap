pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import Yap.Ui

// Who reacted to a message (the desktop's ReactionDetailsDialog on the same
// `Extras.reactions` view): a tab per emoji with its count, and the people
// on the chosen one. A tap on someone asks for their profile.
BottomSheet {
    id: sheet

    readonly property var view: Extras.reactions.length > 0 ? JSON.parse(Extras.reactions) : ({})
    readonly property string othersLine: view.others ?? ""
    readonly property var tabs: view.tabs ?? []
    readonly property var people: view.people ?? []

    signal profileRequested(string userId)

    function openFor(messageId, emoji) {
        Extras.showReactions(messageId, emoji)
        open()
    }

    title: qsTr("Reactions")
    contentHeight: tabRow.height + 8 + Math.max(3, people.length) * 56 + (othersLine.length > 0 ? 40 : 0)
    flickable: list
    onClosed: Extras.closeReactions()

    ListView {
        id: tabRow
        x: 16
        width: parent.width - 32
        height: Theme.controlLg
        orientation: ListView.Horizontal
        spacing: 8
        clip: true
        model: sheet.tabs
        delegate: Rectangle {
            id: tab
            required property var modelData
            width: tabContent.implicitWidth + 24
            height: Theme.controlLg
            radius: Theme.radius
            color: tab.modelData.selected ? Theme.tint2 : tabTap.pressed ? Theme.tint3 : Theme.tint1
            Accessible.role: Accessible.PageTab
            Accessible.name: tab.modelData.label + " " + tab.modelData.count
            Accessible.selected: tab.modelData.selected
            Row {
                id: tabContent
                anchors.centerIn: parent
                spacing: 8
                EmojiGlyph {
                    anchors.verticalCenter: parent.verticalCenter
                    size: 22
                    emoji: tab.modelData.emoji
                    url: tab.modelData.url
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: tab.modelData.count
                    textFormat: Text.PlainText
                    color: tab.modelData.selected ? Theme.ink1 : Theme.ink3
                    font: Theme.type.num
                }
            }
            // The chosen tab's rule.
            Rectangle {
                visible: tab.modelData.selected
                anchors.bottom: parent.bottom
                width: parent.width
                height: 2
                color: Theme.ink1
            }
            TapHandler {
                id: tabTap
                onTapped: {
                    Native.haptic(Native.Tick)
                    Extras.selectReaction(tab.modelData.emoji)
                }
            }
        }
    }

    ListView {
        id: list
        anchors.top: tabRow.bottom
        anchors.topMargin: 8
        anchors.bottom: parent.bottom
        width: parent.width
        clip: true
        model: sheet.people
        delegate: Item {
            id: person
            required property var modelData
            width: list.width
            height: 56
            Accessible.role: Accessible.ListItem
            Accessible.name: person.modelData.name
            Rectangle {
                anchors.fill: parent
                color: personTap.pressed ? Theme.tint2 : "transparent"
            }
            Avatar {
                id: face
                x: 16
                anchors.verticalCenter: parent.verticalCenter
                size: 40
                initials: person.modelData.initials
                source: person.modelData.avatar
            }
            Column {
                anchors.left: face.right
                anchors.leftMargin: 12
                anchors.right: parent.right
                anchors.rightMargin: 16
                anchors.verticalCenter: parent.verticalCenter
                Text {
                    width: parent.width
                    text: person.modelData.name
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    color: Theme.ink1
                    font: Theme.type.setting
                }
                Text {
                    width: parent.width
                    visible: person.modelData.username.length > 0
                    text: "@" + person.modelData.username
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
            TapHandler {
                id: personTap
                onTapped: {
                    sheet.close()
                    sheet.profileRequested(person.modelData.userId)
                }
            }
        }
        footer: Text {
            visible: sheet.othersLine.length > 0
            width: list.width
            height: visible ? 40 : 0
            leftPadding: 16
            verticalAlignment: Text.AlignVCenter
            text: sheet.othersLine
            textFormat: Text.PlainText
            color: Theme.ink3
            font: Theme.type.description
        }
        Spinner {
            anchors.centerIn: parent
            visible: sheet.opened && sheet.tabs.length === 0
        }
    }
}
