pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// One pin (PinnedMessagesSidebar.tsx `PinnedMessageItem`): px-4 py-3 on
// the panel ply, tint-1 on hover, a seam below. A 32 px avatar, 12 px gap,
// then the name at 14/600 and the time (today) or the date in 12 ink-3,
// the text at 13/20 with "(edited)", then the attachments. Jump and Unpin
// float at the top right (ply-3 with a line edge) on hover or focus; Shift
// on Unpin skips the confirm.
Item {
    id: root

    required property string messageId
    required property string author
    required property string initials
    required property string avatar
    required property string body
    required property string html
    required property string time
    required property string date
    required property int day
    required property string attachments
    required property int today
    property bool canUnpin: false

    signal jumpRequested(string id)
    // `confirm` false: Shift was held.
    signal unpinRequested(string id, string author, string text, bool confirm)
    signal imageOpened(var images, int index)
    signal downloadRequested(var item)
    signal videoOpened(var item)

    readonly property var mediaItems: attachments.length > 0 ? JSON.parse(attachments) : []
    readonly property bool hasText: body.trim().length > 0
    readonly property bool shiftHeld: (hover.point.modifiers & Qt.ShiftModifier) !== 0
    readonly property bool revealed: hover.hovered || jumpKey.activeFocus || unpinKey.activeFocus
    // "Jump to message from Alice: the text…" (pinnedRowLabel).
    function label(verb) {
        const text = body.replace(/\s+/g, " ").trim()
        const snippet = text.length > 60 ? text.slice(0, 60).trimEnd() + "..."
            : text.length > 0 ? text : mediaItems.length > 0 ? qsTr("attachment") : ""
        return snippet.length > 0 ? qsTr("%1 from %2: %3").arg(verb).arg(author).arg(snippet)
            : qsTr("%1 from %2").arg(verb).arg(author)
    }

    implicitHeight: column.y + column.height + 12 + 1
    Accessible.role: Accessible.ListItem
    Accessible.name: author

    HoverHandler { id: hover }

    Rectangle {
        anchors.fill: parent
        color: hover.hovered ? Theme.tint1 : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.dur1 } }
    }

    Avatar {
        x: 16
        y: 12
        size: 32
        initials: root.initials
        source: root.avatar
    }

    Column {
        id: column
        x: 60
        y: 12
        width: root.width - x - 16

        Row {
            spacing: 8
            width: parent.width
            Text {
                id: name
                width: Math.min(implicitWidth, parent.width - stamp.implicitWidth - 8)
                text: root.author
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.panelTitle
            }
            Text {
                id: stamp
                anchors.baseline: name.baseline
                text: root.day === root.today ? root.time : root.date
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
        TextEdit {
            visible: root.hasText
            width: parent.width
            height: root.hasText ? implicitHeight : 0
            readOnly: true
            selectByMouse: true
            wrapMode: TextEdit.Wrap
            textFormat: TextEdit.RichText
            text: Theme.richText(root.html, false)
            color: Theme.ink1
            selectionColor: Theme.selection
            selectedTextColor: Theme.ink1
            font: Theme.type.description
            // Message text is untrusted: only web and mail links leave.
            onLinkActivated: link => {
                if (/^(https?:|mailto:)/i.test(link))
                    Qt.openUrlExternally(link)
            }
            HoverHandler {
                cursorShape: parent.hoveredLink.length > 0 ? Qt.PointingHandCursor : Qt.IBeamCursor
            }
        }
        Loader {
            active: root.mediaItems.length > 0
            visible: active
            sourceComponent: MessageMedia {
                items: root.mediaItems
                maxWidth: Math.min(280, column.width)
                onOpenImage: (images, index) => root.imageOpened(images, index)
                onOpenVideo: item => root.videoOpened(item)
                onDownload: item => root.downloadRequested(item)
            }
        }
    }

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.line
    }

    // Jump and Unpin over the row's top right, revealed on hover or focus.
    Rectangle {
        anchors.right: parent.right
        anchors.rightMargin: 12
        y: 8
        width: keys.implicitWidth + 4
        height: keys.implicitHeight + 4
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.line
        opacity: root.revealed ? 1 : 0
        visible: opacity > 0
        Behavior on opacity { OpacityAnimator { duration: Theme.dur1 } }
        Row {
            id: keys
            anchors.centerIn: parent
            spacing: 2
            KitButton {
                id: jumpKey
                anchors.verticalCenter: parent.verticalCenter
                variant: "ghost"
                size: "sm"
                text: qsTr("Jump")
                Accessible.name: root.label(qsTr("Jump to message"))
                onClicked: root.jumpRequested(root.messageId)
            }
            IconKey {
                id: unpinKey
                anchors.verticalCenter: parent.verticalCenter
                visible: root.canUnpin
                size: 28
                iconName: "close"
                tip: qsTr("Unpin (Shift-click skips the confirm)")
                Accessible.name: root.label(qsTr("Unpin message"))
                onClicked: root.unpinRequested(root.messageId, root.author, root.body, !root.shiftHeld)
            }
        }
    }
}
