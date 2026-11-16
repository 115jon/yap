pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The frame of a Server settings page (AdminHomePage and the pages it
// opens): the page header with Back, then the content in one scrolling
// column (560 at most, centred on a tablet). While `guarding` (unsaved
// edits), Back asks "Discard changes?" first. Once the open server goes
// (deleted, or we were removed), the page on top goes back to the drawer;
// `watchChannel` pages close once their channel goes.
Rectangle {
    id: root

    property string title
    default property alias content: body.data
    // Sheets over the page (each `anchors.fill: parent`).
    property alias overlays: overlayLayer.data
    // Unsaved edits: Back asks first.
    property bool guarding: false
    property string discardBody: qsTr("Your edits have not been saved.")
    // Close once the settings' channel is gone (a channel's pages).
    property bool watchChannel: false
    // A request on its way holds Back (Main.goBack).
    property bool holdsBack: false
    readonly property bool sheet: false
    property real backProgress: 0
    readonly property alias scroll: scroll

    readonly property var model: JSON.parse(Admin.state)
    readonly property var server: model.server
    readonly property var channel: model.channel

    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: Math.max(SafeArea.margins.bottom, Native.gestureInset)
    readonly property real keyboardPad: Math.max(bottomInset, Native.imeHeight)

    signal closeRequested()
    // Back to the drawer (Main.wire): the server is gone.
    signal serverOpened()

    function requestBack() {
        if (holdsBack)
            return
        if (!guarding) {
            closeRequested()
            return
        }
        Qt.inputMethod.hide()
        discardSheet.open()
    }
    // Keeps `item` in view above the keyboard (a field being typed in).
    function reveal(item) {
        if (!item)
            return
        const at = item.mapToItem(body, 0, 0).y
        const room = scroll.height - Native.imeHeight
        if (at + item.height > scroll.contentY + room - 16)
            scroll.contentY = Math.max(0, at + item.height - room + 16)
    }

    // Unsaved edits hold Back: it asks first. Only while on top, so a
    // page pushed over this one keeps its own Back.
    // Activating too: a second Back while the page over this one leaves
    // still asks first.
    readonly property bool holdsLayer: guarding && (StackView.status === StackView.Active || StackView.status === StackView.Activating)
    onHoldsLayerChanged: holdsLayer ? Native.pushBackLayer(root) : Native.popBackLayer(root)
    Component.onDestruction: Native.popBackLayer(root)
    function backProgressed(progress) {}
    function backCancelled() {}
    function backInvoked() {
        requestBack()
    }

    // The server (or the channel) showed once; when it goes, only the page
    // on top answers, so a stack closing all at once acts once.
    property bool serverSeen: false
    property bool channelSeen: false
    readonly property bool onTop: StackView.status === StackView.Active
    onServerChanged: {
        if (server)
            serverSeen = true
        else if (serverSeen && onTop)
            serverOpened()
    }
    function checkChannel() {
        if (!watchChannel)
            return
        if (channel)
            channelSeen = true
        else if (channelSeen && onTop && model.server)
            closeRequested()
    }
    onChannelChanged: checkChannel()
    // A page under the one that closed checks again once it is on top
    // (later: the check reads `onTop`).
    function checkOnTop() {
        if (!onTop)
            return
        if (serverSeen && !model.server)
            serverOpened()
        else
            checkChannel()
    }
    onOnTopChanged: Qt.callLater(checkOnTop)
    Component.onCompleted: {
        serverSeen = server !== null
        channelSeen = channel !== null
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

    // Nothing under the page takes a touch.
    MouseArea { anchors.fill: parent }

    PageHeader {
        id: header
        z: 1
        title: root.title
        topInset: root.topInset
        leftInset: root.leftInset
        rightInset: root.rightInset
        onBackRequested: root.requestBack()
    }

    Flickable {
        id: scroll
        anchors.top: header.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        contentHeight: body.height + root.keyboardPad
        clip: true
        boundsBehavior: Flickable.StopAtBounds

        Column {
            id: body
            x: Math.max(16 + root.leftInset, (scroll.width - width) / 2)
            width: Math.min(scroll.width - 32 - root.leftInset - root.rightInset, 560)
            topPadding: 16
            bottomPadding: 32
            spacing: 24
        }
    }

    Item {
        id: overlayLayer
        anchors.fill: parent
        z: 2
    }

    ConfirmSheet {
        id: discardSheet
        anchors.fill: parent
        z: 3
        title: qsTr("Discard changes?")
        body: root.discardBody
        confirmText: qsTr("Discard")
        danger: true
        onConfirmed: root.closeRequested()
    }
}
