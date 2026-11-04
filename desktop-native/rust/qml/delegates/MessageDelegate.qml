import QtQuick
import com.yap.native

// One message (MessageItem.tsx): pl-4 pr-6, a 40 px avatar, 12 px gap, so
// the text column starts at x=68. A new author group adds 16 px above;
// every row has 2 px vertical padding. A grouped message (same author, a
// few minutes later) drops the avatar and name and shows its time in the
// gutter on hover. Every value is a MessageListModel role, already
// formatted.
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

    readonly property string dayText: day === today ? qsTr("Today at %1").arg(time) : date + " · " + time
    readonly property int padTop: grouped ? 2 : 16 + 2

    // A pooled row must not carry a selection into the row it is reused for.
    ListView.onPooled: bodyText.deselect()

    implicitHeight: bodyText.y + bodyText.height + (attachments.visible ? attachments.height + 4 : 0) + 2
    color: hover.hovered ? Theme.tint1 : "transparent"

    HoverHandler { id: hover }

    Avatar {
        visible: !root.grouped
        x: 16
        y: root.padTop + 2
        size: 40
        initials: root.initials
        source: root.avatar
    }

    Text {
        // Gutter time for grouped messages: Martian 11, right-aligned.
        visible: root.grouped && hover.hovered
        x: 16
        width: 40
        y: bodyText.y + 4
        horizontalAlignment: Text.AlignRight
        text: root.time
        color: Theme.ink3
        font: Theme.type.numSm
    }

    Row {
        id: meta
        visible: !root.grouped
        x: 68
        y: root.padTop
        spacing: 8
        Text {
            id: authorText
            text: root.author
            color: Theme.ink1
            font: Theme.type.author
        }
        Text {
            anchors.baseline: authorText.baseline
            text: root.dayText
            color: Theme.ink3
            font: Theme.type.numSm
        }
    }

    TextEdit {
        id: bodyText
        x: 68
        y: root.grouped ? root.padTop : meta.y + Theme.type.authorLine
        width: root.width - x - 24
        readOnly: true
        selectByMouse: true
        wrapMode: TextEdit.Wrap
        textFormat: TextEdit.MarkdownText
        text: root.body + (root.edited ? "  *(edited)*" : "")
        color: Theme.ink1
        selectionColor: Theme.selection
        selectedTextColor: Theme.ink1
        font: Theme.type.message
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

    // File chip (until the embed file card): 28 high, glyph plus name.
    Rectangle {
        id: attachments
        visible: root.attachmentNames.length > 0
        x: 68
        y: bodyText.y + bodyText.height + 4
        height: 28
        width: attachmentRow.implicitWidth + 16
        radius: Theme.radius
        color: Theme.tint1
        border.width: 1
        border.color: Theme.line
        Row {
            id: attachmentRow
            anchors.centerIn: parent
            spacing: 6
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: "file"
                size: Theme.iconSm
                color: Theme.ink3
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.attachmentNames
                color: Theme.ink2
                font: Theme.type.description
            }
        }
    }
}
