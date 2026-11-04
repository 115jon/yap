import QtQuick
import com.yap.native

// One message. A grouped message (same author, a few minutes later) drops the
// avatar and name and shows its time in the gutter on hover. Every value is
// a MessageListModel role, already formatted.
Rectangle {
    id: root

    required property string authorId
    required property string author
    required property string initials
    required property string avatar
    required property string body
    required property string time
    required property string date
    required property int day
    required property bool edited
    required property bool grouped
    required property string attachmentNames
    // Today's local day number (`Chat.today`).
    required property int today

    readonly property string dayText: day === today ? "Today at " + time : date + " · " + time

    // A pooled row must not carry a selection into the row it is reused for.
    ListView.onPooled: bodyText.deselect()

    implicitHeight: bodyText.y + bodyText.height + (attachments.visible ? attachments.height + 4 : 0) + 4
    color: hover.hovered ? Theme.ply1 : "transparent"

    HoverHandler { id: hover }

    Avatar {
        visible: !root.grouped
        x: 20
        y: 4
        size: 40
        initials: root.initials
        fillKey: root.authorId
        source: root.avatar
    }

    Text {
        // Gutter time for grouped messages.
        visible: root.grouped && hover.hovered
        x: 20
        width: 40
        y: bodyText.y + 3
        horizontalAlignment: Text.AlignHCenter
        text: root.time
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
            id: authorText
            text: root.author
            color: Theme.ink1
            font.family: Theme.display
            font.pixelSize: 15
            font.weight: Font.Bold
        }
        Text {
            anchors.baseline: authorText.baseline
            text: root.dayText
            color: Theme.ink3
            font.family: Theme.mono
            font.pixelSize: 11
        }
    }

    TextEdit {
        id: bodyText
        x: 76
        y: root.grouped ? 2 : meta.y + 22
        width: root.width - x - 24
        readOnly: true
        selectByMouse: true
        wrapMode: TextEdit.Wrap
        textFormat: TextEdit.MarkdownText
        text: root.body + (root.edited ? "  *(edited)*" : "")
        color: Theme.ink1
        selectionColor: "#4d4cc2ff"
        selectedTextColor: Theme.ink1
        font.family: Theme.text
        font.pixelSize: 15
        // Message text is untrusted: only open web and mail links. On Windows
        // openUrlExternally is ShellExecute, so file:// etc. would run things.
        onLinkActivated: link => {
            if (/^(https?:|mailto:)/i.test(link))
                Qt.openUrlExternally(link)
        }

        HoverHandler {
            cursorShape: bodyText.hoveredLink.length > 0 ? Qt.PointingHandCursor : Qt.IBeamCursor
        }
    }

    Rectangle {
        id: attachments
        visible: root.attachmentNames.length > 0
        x: 76
        y: bodyText.y + bodyText.height + 4
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
