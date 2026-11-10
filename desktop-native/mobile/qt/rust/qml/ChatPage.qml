import QtQuick
import Yap.Mobile
import com.yap.native

// The open channel, phone layout: a header with the back key, the
// desktop's MessageDelegate rows on its MessageListModel, the typing line
// and the composer. The keyboard pushes the composer up as it animates in
// (Native.imeHeight follows the IME inset frame by frame).
Item {
    id: root

    // The predictive back gesture's progress (0..1): the page follows the
    // finger before the pop commits, as an Android activity does.
    property real backProgress: 0

    signal back()

    readonly property real keyboard: Native.imeHeight
    readonly property real bottomInset: Math.max(SafeArea.margins.bottom, keyboard)

    // Follow the newest message unless the user scrolled away from it.
    property bool followEnd: true

    x: backProgress * width * 0.1
    scale: 1 - backProgress * 0.08
    opacity: 1 - backProgress * 0.2

    Rectangle {
        anchors.fill: parent
        color: Theme.ply0
    }

    Rectangle {
        id: header
        width: parent.width
        height: SafeArea.margins.top + 56
        color: Theme.ply0
        z: 2

        Item {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 56

            IconKey {
                id: backKey
                anchors.left: parent.left
                anchors.leftMargin: 8 + SafeArea.margins.left
                anchors.verticalCenter: parent.verticalCenter
                iconName: "back"
                tip: qsTr("Back")
                onClicked: root.back()
            }
            Text {
                anchors.left: backKey.right
                anchors.leftMargin: 8
                anchors.right: link.left
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                text: "#" + Nav.activeChannelName
                color: Theme.ink1
                font: Theme.type.columnTitle
                elide: Text.ElideRight
            }
            Led {
                id: link
                anchors.right: parent.right
                anchors.rightMargin: 16 + SafeArea.margins.right
                anchors.verticalCenter: parent.verticalCenter
                color: Nav.connected ? Theme.talk : Theme.idle
                lit: true
            }
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
    }

    PlatformList {
        id: view
        anchors.top: header.bottom
        anchors.bottom: typing.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: SafeArea.margins.left
        anchors.rightMargin: SafeArea.margins.right
        clip: true
        bottomMargin: 8
        model: MessageListModel

        // The desktop's day rule (ChatView's section delegate, condensed).
        section.property: "day"
        section.criteria: ViewSection.FullString
        section.delegate: Item {
            required property string section
            readonly property int dayNumber: Number(section)
            readonly property var dayDate: new Date(1970, 0, 1 + dayNumber)
            width: view.width
            height: 32
            Rectangle {
                id: rule
                x: 16
                anchors.verticalCenter: parent.verticalCenter
                width: 24
                height: 2
                color: Theme.ink1
            }
            Text {
                anchors.left: rule.right
                anchors.leftMargin: 12
                anchors.verticalCenter: rule.verticalCenter
                text: parent.dayNumber === Chat.today ? qsTr("Today")
                    : parent.dayNumber === Chat.today - 1 ? qsTr("Yesterday")
                    : Qt.formatDate(parent.dayDate, "dddd, MMMM d")
                color: Theme.ink3
                font: Theme.type.silk
            }
        }

        delegate: MessageDelegate {
            id: row
            width: view.width
            // Touch has no right click: a long press opens the share sheet
            // with the message's text (the desktop's menu is not on mobile).
            TapHandler {
                acceptedDevices: PointerDevice.TouchScreen
                onLongPressed: {
                    Native.haptic(Native.LongPress)
                    Native.share(row.body)
                }
            }
            today: Chat.today
            editingId: ""
            highlightId: ""
            onRetryRequested: nonce => Chat.retrySend(nonce)
            onDiscardRequested: (nonce, text) => Chat.discardSend(nonce)
            onReactionToggled: (id, emoji) => {
                Native.haptic(Native.Tick)
                Chat.toggleReaction(id, emoji)
            }
            onLinkOpened: url => Qt.openUrlExternally(url)
        }

        function nearEnd() {
            return contentHeight - (contentY - originY) - height < 48
        }

        onMovementStarted: root.followEnd = false
        onMovementEnded: root.followEnd = nearEnd()
        onContentHeightChanged: if (root.followEnd) positionViewAtEnd()
        onHeightChanged: if (root.followEnd) positionViewAtEnd()
        onContentYChanged: {
            if (contentY - originY < 600 && Chat.hasOlder && !Chat.olderLoading && moving)
                Chat.loadOlder()
            Chat.setAtBottom(nearEnd())
        }
        onCountChanged: if (root.followEnd) positionViewAtEnd()

        Connections {
            target: MessageListModel
            // A send of ours always lands at the end, as on the desktop.
            function onAppended(mine) {
                if (mine)
                    root.followEnd = true
                if (root.followEnd)
                    view.positionViewAtEnd()
            }
        }
    }

    Spinner {
        anchors.centerIn: view
        visible: Chat.pageLoading && view.count === 0
    }

    Text {
        id: typing
        anchors.bottom: composer.top
        x: 16 + SafeArea.margins.left
        width: parent.width - 32
        height: Chat.typingText.length > 0 ? 20 : 0
        text: Chat.typingText
        color: Theme.ink3
        font: Theme.type.caption
        elide: Text.ElideRight
        Behavior on height { NumberAnimation { duration: Theme.dur2 } }
    }

    MobileComposer {
        id: composer
        anchors.bottom: parent.bottom
        anchors.bottomMargin: root.bottomInset
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: SafeArea.margins.left
        anchors.rightMargin: SafeArea.margins.right
        placeholder: qsTr("Message #%1").arg(Nav.activeChannelName)
        enabled: !Chat.readOnly
        onSend: text => {
            Native.haptic(Native.Confirm)
            root.followEnd = true
            Chat.sendMessage(text, "")
        }
    }

    // Open at the newest message once the first rows have laid out.
    Component.onCompleted: Qt.callLater(view.positionViewAtEnd)
}
