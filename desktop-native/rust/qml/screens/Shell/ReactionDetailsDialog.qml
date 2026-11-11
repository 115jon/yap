pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native
import Yap.Ui

// Who reacted (ReactionDetailsModal.tsx): a 400 wide dialog, "Reactions",
// emoji tabs across the top (a 20 px emoji and its count, the selected one
// underlined in ink-1) over a seam, then 36 px people rows (24 px avatar,
// the name at 500 and the @handle in ink-3; with Manage messages a remove
// key on hover). When the server named fewer people than the count, the
// list says "and N others". It closes itself when the last reaction goes.
Popup {
    id: root

    readonly property var view: JSON.parse(Extras.reactions)

    function ask(messageId, emoji) {
        Extras.showReactions(messageId, emoji)
        open()
    }

    parent: Overlay.overlay
    anchors.centerIn: parent
    width: 400
    modal: true
    // Ctrl+K stays shut while a modal is up (ExtrasHost).
    onVisibleChanged: ShellState.modalCount += visible ? 1 : -1
    focus: true
    padding: 20
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onClosed: Extras.closeReactions()

    // The last reaction went (or its message): nothing left to show.
    Connections {
        target: Extras
        function onReactionsChanged() {
            if (root.opened && !root.view.open)
                root.close()
        }
    }

    Overlay.modal: Scrim { color: Theme.scrim }

    background: Rectangle {
        radius: Theme.radiusSheet
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
    }

    enter: PresenceIn { fromScale: Theme.scaleDialog }
    exit: PresenceOut { toScale: Theme.scaleDialogOut }

    contentItem: Column {
        spacing: 12

        Item {
            width: parent.width
            height: 32
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Reactions")
                color: Theme.ink1
                font: Theme.type.sectionTitle
                Accessible.role: Accessible.Heading
            }
            IconKey {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                iconName: "close"
                size: 32
                tone: "muted"
                tip: qsTr("Close")
                onClicked: root.close()
            }
        }

        // The tabs, scrolling sideways when there are many.
        Item {
            width: parent.width
            height: 37
            Flickable {
                anchors.fill: parent
                contentWidth: tabs.implicitWidth
                flickableDirection: Flickable.HorizontalFlick
                boundsBehavior: Flickable.StopAtBounds
                clip: true
                Row {
                    id: tabs
                    height: 36
                    spacing: 2
                    Accessible.role: Accessible.PageTabList
                    Accessible.name: qsTr("Reactions by emoji")
                    Repeater {
                        model: root.view.tabs
                        ReactionTab {}
                    }
                }
            }
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 1
                color: Theme.line
            }
        }

        ListView {
            id: people
            width: parent.width
            height: Math.min(324, contentHeight)
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            model: root.view.people
            delegate: PersonRow {}
            footer: Text {
                visible: root.view.others.length > 0
                height: visible ? 36 : 0
                leftPadding: 8
                verticalAlignment: Text.AlignVCenter
                textFormat: Text.PlainText
                text: root.view.others
                color: Theme.ink3
                font: Theme.type.description
            }
            ScrollBar.vertical: ThinScrollBar {}
        }
    }

    component ReactionTab: AbstractButton {
        id: tab
        required property var modelData
        height: 36
        leftPadding: 8
        rightPadding: 8
        hoverEnabled: true
        Accessible.role: Accessible.PageTab
        Accessible.name: modelData.label + " " + modelData.count
        Accessible.selected: modelData.selected
        onClicked: Extras.selectReaction(modelData.emoji)
        contentItem: Row {
            spacing: 6
            Image {
                anchors.verticalCenter: parent.verticalCenter
                visible: tab.modelData.url.length > 0 && status !== Image.Error
                width: 20
                height: 20
                sourceSize: Qt.size(40, 40)
                asynchronous: true
                source: tab.modelData.url
                mipmap: true
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                visible: tab.modelData.url.length === 0
                width: Math.min(implicitWidth, 96)
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: tab.modelData.label
                color: tab.modelData.selected ? Theme.ink1 : Theme.ink3
                font: Theme.type.description
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: tab.modelData.count
                color: tab.modelData.selected || tab.hovered ? Theme.ink1 : Theme.ink3
                EaseColor on color {}
                font: tab.modelData.selected ? Theme.type.kbd : Theme.type.num
            }
        }
        background: Item {
            Rectangle {
                anchors.fill: parent
                radius: Theme.radius
                color: Theme.tint1
                visible: tab.hovered && !tab.modelData.selected
            }
            Rectangle {
                x: 4
                anchors.bottom: parent.bottom
                anchors.bottomMargin: -1
                width: parent.width - 8
                height: 2
                radius: 1
                color: Theme.ink1
                opacity: tab.modelData.selected ? 1 : 0
                Behavior on opacity { NumberAnimation { duration: Theme.dur2 } }
            }
            Rectangle {
                anchors.fill: parent
                visible: tab.visualFocus
                color: "transparent"
                radius: Theme.radius
                border.width: 2
                border.color: Theme.focus
            }
        }
        HoverHandler { cursorShape: Qt.PointingHandCursor }
    }

    component PersonRow: Item {
        id: person
        required property var modelData
        width: ListView.view.width
        height: 36
        Accessible.role: Accessible.ListItem
        Accessible.name: modelData.name
        HoverHandler { id: personHover }
        Rectangle {
            anchors.fill: parent
            radius: Theme.radius
            color: personHover.hovered ? Theme.tint1 : "transparent"
            EaseColor on color {}
        }
        Avatar {
            id: face
            x: 8
            anchors.verticalCenter: parent.verticalCenter
            size: 24
            initials: person.modelData.initials
            source: person.modelData.avatar
        }
        Row {
            anchors.left: face.right
            anchors.leftMargin: 10
            anchors.right: remove.left
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 6
            Text {
                id: name
                width: Math.min(implicitWidth, parent.width * 0.6)
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: person.modelData.name
                color: Theme.ink1
                font: Theme.type.row
            }
            Text {
                anchors.baseline: name.baseline
                visible: person.modelData.username.length > 0
                width: Math.min(implicitWidth, parent.width - name.width - 6)
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: "@" + person.modelData.username
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
        IconKey {
            id: remove
            anchors.right: parent.right
            anchors.rightMargin: 4
            anchors.verticalCenter: parent.verticalCenter
            visible: root.view.canManage && (personHover.hovered || activeFocus)
            iconName: "close"
            size: 24
            tone: "ghost"
            tip: qsTr("Remove reaction from %1").arg(person.modelData.name)
            onClicked: {
                const tab = root.view.tabs.find(t => t.selected)
                if (tab)
                    Extras.removeReaction(tab.emoji, person.modelData.userId)
            }
        }
    }
}
