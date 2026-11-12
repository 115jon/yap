pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// A message's thread (the desktop's ThreadPanel, phone layout), a page over
// the pager: "Thread" with "N replies", the root message over a seam and the
// replies label (or "No replies yet"), the replies as the chat's rows on
// the shared ThreadMessageModel, then a reply field. A long press opens the
// message sheet, as in the chat; its Reply goes to the thread's field.
// Loading shows the spinner; a failure says so with Try again.
Rectangle {
    id: root

    required property string rootId
    readonly property bool sheet: false
    property real backProgress: 0

    signal closeRequested()
    signal profileRequested(string userId)

    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: Math.max(SafeArea.margins.bottom, Native.gestureInset, Native.imeHeight)

    readonly property bool ready: !Panels.threadLoading && Panels.threadError.length === 0
        && Panels.threadRootId === rootId
    readonly property string repliesText: Panels.threadReplies === 1
        ? qsTr("1 reply") : qsTr("%1 replies").arg(Panels.threadReplies)

    property string editingId: ""
    // The core closed this thread under the page (another one opened).
    property bool lost: false
    Connections {
        target: Panels
        function onThreadRootIdChanged() {
            root.lost = Panels.threadRootId !== root.rootId
        }
    }

    function load() {
        editingId = ""
        Panels.openThread(rootId)
    }
    function send() {
        const text = field.text.trim()
        if (text.length === 0)
            return
        Panels.sendReply(text)
        field.text = ""
    }

    color: Theme.ply0
    transform: [
        Scale {
            origin.x: root.width / 2
            origin.y: root.height / 2
            xScale: 1 - root.backProgress * 0.08
            yScale: 1 - root.backProgress * 0.08
        },
        Translate { x: root.backProgress * root.width * 0.1 }
    ]

    Component.onCompleted: load()
    Component.onDestruction: Panels.closeThread()

    PageHeader {
        id: header
        z: 2
        title: qsTr("Thread")
        topInset: root.topInset
        leftInset: root.leftInset
        rightInset: root.rightInset
        onBackRequested: root.closeRequested()
        Text {
            visible: root.ready
            anchors.verticalCenter: parent.verticalCenter
            rightPadding: 8
            textFormat: Text.PlainText
            text: root.repliesText
            color: Theme.ink3
            font: Theme.type.caption
        }
    }

    // The root's seam and the replies label, under its row.
    Component {
        id: rootFoot
        Column {
            width: list.width
            Item { width: 1; height: 12 }
            Rectangle {
                width: parent.width
                height: 1
                color: Theme.line
            }
            Text {
                visible: Panels.threadReplies > 0
                x: 16
                topPadding: 12
                bottomPadding: 4
                textFormat: Text.PlainText
                text: root.repliesText
                color: Theme.ink3
                font: Theme.type.caption
            }
            Text {
                visible: Panels.threadReplies === 0
                width: parent.width
                topPadding: 24
                bottomPadding: 24
                horizontalAlignment: Text.AlignHCenter
                textFormat: Text.PlainText
                text: qsTr("No replies yet. Start the thread below.")
                wrapMode: Text.Wrap
                color: Theme.ink3
                font: Theme.type.description
            }
        }
    }

    PlatformList {
        id: list
        anchors.top: header.bottom
        anchors.bottom: composer.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: root.leftInset
        anchors.rightMargin: root.rightInset
        visible: root.ready
        clip: true
        // As the chat's list: still under a sheet (its long press leaves
        // the list pressed, which then took the sheet's drags).
        interactive: !actions.shown
        // As the chat's list: a row has its height before it is placed.
        reuseItems: false
        bottomMargin: 8
        model: ThreadMessageModel

        delegate: MessageDelegate {
            id: row
            width: list.width
            today: Chat.today
            editingId: root.editingId
            highlightId: ""
            threadActions: false
            foot: messageId === Panels.threadRootId ? rootFoot : null
            onMenuRequested: actions.openFor(row)
            onReplyRequested: field.forceActiveFocus()
            onEditRequested: id => root.editingId = id
            onEditSaved: (id, text) => {
                root.editingId = ""
                Qt.inputMethod.hide()
                if (text.trim().length === 0)
                    actions.confirmDelete(row)
                else
                    Chat.editMessage(id, text)
            }
            onEditCancelled: {
                root.editingId = ""
                Qt.inputMethod.hide()
            }
            onProfileRequested: (userId, anchor) => root.profileRequested(userId)
            onReactionToggled: (id, emoji) => actions.react(id, emoji)
            onReactionPickerRequested: (id, anchor, toRight) => actions.pickReaction(id)
            onReactionDetailsRequested: (id, emoji) => actions.showReactions(id, emoji)
            onLinkOpened: url => {
                if (/^(https?:|mailto:)/i.test(url))
                    Qt.openUrlExternally(url)
            }
            onImageOpened: (images, index) => viewer.show(images, index)
        }
    }

    // New replies scroll into view (ThreadSidebar.tsx), and the keyboard
    // keeps the newest one above the field (Yap.Ui's ChatScroll, as the
    // desktop's thread panel).
    ChatScroll {
        view: list
        thread: true
        endSlack: 56
    }

    Spinner {
        anchors.centerIn: list
        running: Panels.threadLoading
    }

    // The load failed (or this thread was closed under the page): what
    // went wrong, and Try again.
    Column {
        visible: Panels.threadError.length > 0 || (root.lost && !Panels.threadLoading)
        anchors.top: header.bottom
        anchors.topMargin: 16
        x: 16 + root.leftInset
        width: parent.width - 32 - root.leftInset - root.rightInset
        spacing: 12
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: Panels.threadError.length > 0 ? Panels.threadError : qsTr("This thread is not loaded.")
            wrapMode: Text.Wrap
            color: Theme.dangerInk
            font: Theme.type.description
            Accessible.role: Accessible.AlertMessage
            Accessible.name: text
        }
        KitButton {
            variant: "secondary"
            text: qsTr("Try again")
            onClicked: root.load()
        }
    }

    // The reply field and its Send key, over the keyboard.
    Rectangle {
        id: composer
        anchors.bottom: parent.bottom
        anchors.bottomMargin: root.bottomInset
        width: parent.width
        height: field.height + 16
        color: Theme.ply0
        Rectangle {
            width: parent.width
            height: 1
            color: Theme.line
        }
        MobileField {
            id: field
            x: 12 + root.leftInset
            y: 8
            width: parent.width - 24 - root.leftInset - root.rightInset - sendKey.width - 4
            enabled: root.ready
            placeholderText: qsTr("Reply to thread…")
            Accessible.name: qsTr("Reply to thread")
            onAccepted: root.send()
        }
        IconKey {
            id: sendKey
            focusPolicy: Qt.NoFocus
            anchors.left: field.right
            anchors.leftMargin: 4
            anchors.verticalCenter: field.verticalCenter
            iconName: "send"
            tip: qsTr("Send")
            enabled: root.ready && (field.text + field.preeditText).trim().length > 0
            onClicked: {
                // The keyboard's word in progress counts.
                Qt.inputMethod.commit()
                root.send()
            }
        }
    }

    // The message sheet and what it leads to.
    MessageActions {
        id: actions
        anchors.fill: parent
        z: 9
        onReplyRequested: (messageId, author, text) => field.forceActiveFocus()
        onEditRequested: messageId => root.editingId = messageId
        onProfileRequested: userId => root.profileRequested(userId)
    }

    // A picture in the thread, over the page. Back closes it first.
    ImageViewerPage {
        id: viewer
        anchors.fill: parent
        z: 10
        onShownChanged: {
            if (shown) {
                Native.holdDarkBars(viewer)
                Native.pushBackLayer(viewerBack)
            } else {
                Native.releaseDarkBars(viewer)
                Native.popBackLayer(viewerBack)
            }
        }
        Component.onDestruction: {
            Native.releaseDarkBars(viewer)
            Native.popBackLayer(viewerBack)
        }
    }
    // Native's back layer for the viewer (as a sheet's).
    QtObject {
        id: viewerBack
        function backProgressed(progress) {
            viewer.backProgress = progress
        }
        function backCancelled() {
            viewer.backProgress = 0
        }
        function backInvoked() {
            viewer.backProgress = 0
            viewer.close()
        }
    }
}
