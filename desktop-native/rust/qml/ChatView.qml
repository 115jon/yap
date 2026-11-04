import QtQuick
import QtQuick.Controls.Basic

// One text channel: header, live message list, typing line, composer.
Rectangle {
    id: root

    required property var client
    property var me

    color: Theme.ply0

    // Messages from the same author within this window share one header.
    readonly property int groupWindowMs: 5 * 60 * 1000

    ListModel { id: messages }

    function groupedWith(previous, message) {
        if (!previous)
            return false
        return previous.authorId === message.authorId
            && Date.parse(message.createdAt) - Date.parse(previous.createdAt) < groupWindowMs
    }

    function row(message, previous) {
        return {
            messageId: message.id,
            authorId: message.authorId || "",
            author: message.author || "",
            avatar: message.avatar || "",
            content: message.content || "",
            createdAt: message.createdAt || "",
            edited: !!message.edited,
            attachmentNames: (message.attachments || []).map(a => a.name).join("  ·  "),
            grouped: groupedWith(previous, message)
        }
    }

    function indexOf(id) {
        for (let i = messages.count - 1; i >= 0; i--)
            if (messages.get(i).messageId === id)
                return i
        return -1
    }

    function regroup(i) {
        if (i >= 0 && i < messages.count)
            messages.setProperty(i, "grouped", groupedWith(i > 0 ? messages.get(i - 1) : null, messages.get(i)))
    }

    Connections {
        target: root.client

        function onMessagesReset(json) {
            const list = JSON.parse(json)
            messages.clear()
            let previous = null
            for (const m of list) {
                messages.append(root.row(m, previous))
                previous = m
            }
            Qt.callLater(() => view.positionViewAtEnd())
        }

        function onMessageUpserted(json) {
            const m = JSON.parse(json)
            const i = root.indexOf(m.id)
            if (i >= 0) {
                if (typeof m.content === "string")
                    messages.setProperty(i, "content", m.content)
                if (m.edited !== undefined)
                    messages.setProperty(i, "edited", !!m.edited)
                return
            }
            if (m.partial)
                return
            const stick = view.atYEnd
            messages.append(root.row(m, messages.count > 0 ? messages.get(messages.count - 1) : null))
            typingLine.clear(m.author)
            if (stick || m.authorId === (root.me ? root.me.id : ""))
                Qt.callLater(() => view.positionViewAtEnd())
        }

        function onMessageRemoved(id) {
            const i = root.indexOf(id)
            if (i >= 0) {
                messages.remove(i)
                root.regroup(i)
            }
        }

        function onTyping(name) {
            typingLine.show(name)
        }

        function onActiveChannelIdChanged() {
            typingLine.names = []
        }
    }

    Item {
        id: header
        width: parent.width
        height: Theme.header

        Row {
            anchors.left: parent.left
            anchors.leftMargin: 20
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            Text {
                text: "#"
                color: Theme.inkOff
                font.family: Theme.mono
                font.pixelSize: 18
                anchors.verticalCenter: parent.verticalCenter
            }
            Text {
                text: root.client.activeChannelName
                color: Theme.ink1
                font.family: Theme.display
                font.pixelSize: 18
                font.weight: Font.ExtraBold
                font.letterSpacing: -0.4
                anchors.verticalCenter: parent.verticalCenter
            }
        }
        Text {
            anchors.right: parent.right
            anchors.rightMargin: 20
            anchors.verticalCenter: parent.verticalCenter
            visible: root.client.status.length > 0
            text: root.client.status
            color: Theme.ink3
            font.family: Theme.text
            font.pixelSize: 12
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.ply2
        }
    }

    ListView {
        id: view
        anchors.top: header.bottom
        anchors.bottom: typingLine.top
        anchors.left: parent.left
        anchors.right: parent.right
        clip: true
        model: messages
        boundsBehavior: Flickable.StopAtBounds
        reuseItems: true
        topMargin: 16
        bottomMargin: 8
        delegate: MessageDelegate { width: view.width }

        ScrollBar.vertical: ScrollBar {
            policy: ScrollBar.AsNeeded
            contentItem: Rectangle {
                implicitWidth: 6
                radius: 3
                color: Theme.ply4
            }
        }

        Text {
            anchors.centerIn: parent
            visible: messages.count === 0 && root.client.activeChannelId.length > 0
            text: "No messages yet. Say something."
            color: Theme.ink3
            font.family: Theme.text
            font.pixelSize: 15
        }
    }

    // "Name is typing…", cleared after 8 s or when they post.
    Item {
        id: typingLine
        property var names: []
        anchors.bottom: composer.top
        width: parent.width
        height: 24

        function show(name) {
            if (names.indexOf(name) < 0)
                names = names.concat([name])
            expiry.restart()
        }
        function clear(name) {
            names = names.filter(n => n !== name)
        }

        Timer { id: expiry; interval: 8000; onTriggered: typingLine.names = [] }

        Text {
            anchors.left: parent.left
            anchors.leftMargin: 20
            anchors.verticalCenter: parent.verticalCenter
            visible: typingLine.names.length > 0
            text: typingLine.names.join(", ") + (typingLine.names.length > 1 ? " are typing…" : " is typing…")
            color: Theme.ink3
            font.family: Theme.text
            font.pixelSize: 12
        }
    }

    Composer {
        id: composer
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.margins: 16
        anchors.topMargin: 0
        channelName: root.client.activeChannelName
        enabled: root.client.activeChannelId.length > 0
        onSend: text => root.client.sendMessage(text)
    }
}
