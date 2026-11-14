pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// The member list's Threads tab (MemberList.tsx ThreadsTabContent): the
// open channel's threads by latest reply, loaded afresh each time the tab
// shows. Each is a ply-2 card (line border, 8 px corners, 12 padding): the
// starter's 20 px avatar and name, up to two lines of it in ink-2, then
// the reply count and when the last reply came. Loading is five static
// bars; a failure says why with Try again; none says so. A card opens its
// thread.
Item {
    id: root

    signal openRequested(string messageId)

    readonly property var rows: JSON.parse(Panels.threads)
    readonly property bool failed: Panels.threadsError.length > 0

    function load() {
        Panels.listThreads(Date.now())
    }

    Component.onCompleted: load()

    // TabSpinnerState: five 56 px bars on tint-1, 8 apart.
    Column {
        visible: Panels.threadsLoading
        y: 8
        width: parent.width
        spacing: 8
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("Loading")
        Repeater {
            model: 5
            delegate: Rectangle {
                width: parent.width
                height: 56
                radius: Theme.radiusSheet
                color: Theme.tint1
            }
        }
    }

    // TabErrorState.
    Column {
        visible: !Panels.threadsLoading && root.failed
        width: parent.width
        topPadding: 48
        spacing: 12
        Accessible.role: Accessible.AlertMessage
        Accessible.name: Panels.threadsError
        Icon {
            anchors.horizontalCenter: parent.horizontalCenter
            name: "offline"
            size: Theme.icon2xl
            color: Theme.ink3
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: qsTr("Unable to load this tab")
            color: Theme.dangerInk
            font: Theme.type.panelTitle
        }
        Text {
            width: parent.width
            leftPadding: 24
            rightPadding: 24
            horizontalAlignment: Text.AlignHCenter
            textFormat: Text.PlainText
            text: Panels.threadsError
            wrapMode: Text.Wrap
            color: Theme.ink2
            font: Theme.type.description
        }
        KitButton {
            anchors.horizontalCenter: parent.horizontalCenter
            variant: "secondary"
            size: "sm"
            iconName: "refresh"
            text: qsTr("Try again")
            onClicked: root.load()
        }
    }

    // TabEmptyState.
    Column {
        visible: !Panels.threadsLoading && !root.failed && root.rows.length === 0
        width: parent.width
        topPadding: 48
        spacing: 12
        Icon {
            anchors.horizontalCenter: parent.horizontalCenter
            name: "chat"
            size: Theme.icon2xl
            color: Theme.ink3
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: qsTr("No threads in this channel")
            color: Theme.ink1
            font: Theme.type.panelTitle
        }
    }

    ListView {
        id: list
        anchors.fill: parent
        visible: !Panels.threadsLoading && !root.failed && count > 0
        spacing: 10
        bottomMargin: 40
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        model: root.rows
        Accessible.role: Accessible.List
        Accessible.name: qsTr("Threads")

        delegate: T.AbstractButton {
            id: card
            required property var modelData
            readonly property string repliesLabel: modelData.replies === 1 ? qsTr("reply") : qsTr("replies")

            width: ListView.view.width
            padding: 12
            // Templates set no implicit size of their own.
            implicitHeight: implicitContentHeight + topPadding + bottomPadding
            focusPolicy: Qt.StrongFocus
            hoverEnabled: true
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Thread by %1, %2 %3").arg(modelData.author).arg(modelData.replies).arg(repliesLabel)
            onClicked: root.openRequested(modelData.id)

            background: Rectangle {
                radius: 8
                color: card.hovered ? Theme.tint1 : Theme.ply2
                border.width: 1
                border.color: card.visualFocus ? Theme.focus : Theme.line
                Behavior on color { ColorAnimation { duration: Theme.dur1 } }
            }
            contentItem: Column {
                Row {
                    spacing: 8
                    width: parent.width
                    Avatar {
                        size: 20
                        initials: card.modelData.initials
                        source: card.modelData.avatar
                        Accessible.ignored: true
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - 28
                        textFormat: Text.PlainText
                        text: card.modelData.author
                        elide: Text.ElideRight
                        color: Theme.ink1
                        font: Theme.type.captionStrong
                    }
                }
                Text {
                    topPadding: 6
                    bottomPadding: 8
                    width: parent.width
                    textFormat: Text.PlainText
                    text: card.modelData.content
                    // An attachment-only parent: the web's empty line box
                    // collapses, leaving only its margins.
                    height: text.trim().length > 0 ? implicitHeight : topPadding + bottomPadding
                    wrapMode: Text.Wrap
                    maximumLineCount: 2
                    elide: Text.ElideRight
                    color: Theme.ink2
                    font: Theme.type.description
                    lineHeight: 1.25
                }
                Row {
                    spacing: 12
                    Row {
                        spacing: 4
                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: "chat"
                            size: Theme.iconXs
                            color: Theme.ink3
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: card.modelData.replies
                            color: Theme.ink3
                            font: Theme.type.captionStrong
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: card.repliesLabel
                            color: Theme.ink3
                            font: Theme.type.caption
                        }
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: card.modelData.lastReply
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                }
            }
            HoverHandler { cursorShape: Qt.PointingHandCursor }
        }

        ScrollBar.vertical: ThinScrollBar {}
    }
}
