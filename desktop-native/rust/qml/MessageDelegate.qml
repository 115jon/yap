import QtQuick

// One message. A grouped message (same author, a few minutes later) drops the
// avatar and name and shows its time in the gutter on hover.
Rectangle {
    id: root

    required property int index
    required property string messageId
    required property string authorId
    required property string author
    required property string avatar
    required property string content
    required property string createdAt
    required property bool edited
    required property string attachmentNames
    required property bool grouped

    readonly property date created: new Date(createdAt)

    implicitHeight: body.y + body.height + (attachments.visible ? attachments.height + 4 : 0) + 4
    color: hover.hovered ? Theme.ply1 : "transparent"

    HoverHandler { id: hover }

    function timeText(d) {
        return d.toLocaleTimeString(Qt.locale(), "h:mm AP")
    }
    function dayText(d) {
        const today = new Date()
        if (d.toDateString() === today.toDateString())
            return "Today at " + timeText(d)
        return d.toLocaleDateString(Qt.locale(), "MMM d") + " · " + timeText(d)
    }

    Avatar {
        id: avatarKey
        visible: !root.grouped
        x: 20
        y: 4
        size: 40
        name: root.author
        key: root.authorId
        source: root.avatar
    }

    Text {
        // Gutter time for grouped messages.
        visible: root.grouped && hover.hovered
        x: 20
        width: 40
        y: body.y + 3
        horizontalAlignment: Text.AlignHCenter
        text: root.timeText(root.created)
        color: Theme.inkOff
        font.family: Theme.mono
        font.pixelSize: 10
    }

    Row {
        id: meta
        visible: !root.grouped
        x: 76
        y: 4
        spacing: 8
        Text {
            text: root.author
            color: Theme.ink1
            font.family: Theme.display
            font.pixelSize: 15
            font.weight: Font.Bold
        }
        Text {
            anchors.baseline: parent.children[0].baseline
            text: root.dayText(root.created)
            color: Theme.ink3
            font.family: Theme.mono
            font.pixelSize: 11
        }
    }

    TextEdit {
        id: body
        x: 76
        y: root.grouped ? 2 : meta.y + 22
        width: root.width - x - 24
        readOnly: true
        selectByMouse: true
        wrapMode: TextEdit.Wrap
        textFormat: TextEdit.MarkdownText
        text: root.content + (root.edited ? "  *(edited)*" : "")
        color: Theme.ink1
        selectionColor: "#4d4cc2ff"
        selectedTextColor: Theme.ink1
        font.family: Theme.text
        font.pixelSize: 15
        onLinkActivated: link => Qt.openUrlExternally(link)

        HoverHandler {
            cursorShape: body.hoveredLink.length > 0 ? Qt.PointingHandCursor : Qt.IBeamCursor
        }
    }

    Rectangle {
        id: attachments
        visible: root.attachmentNames.length > 0
        x: 76
        y: body.y + body.height + 4
        height: 28
        width: attachmentText.implicitWidth + 20
        radius: Theme.radius
        color: Theme.ply2
        border.width: 1
        border.color: Theme.ply4
        Text {
            id: attachmentText
            anchors.centerIn: parent
            text: "FILE  " + root.attachmentNames
            color: Theme.ink2
            font.family: Theme.mono
            font.pixelSize: 11
        }
    }
}
