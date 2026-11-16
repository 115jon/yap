pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The frame of a Settings page (the hub, SettingsPage, and each category
// it opens): the page header with Back, then the content in one scrolling
// column (560 at most, centred on a tablet). Back, Android's back and its
// predictive gesture close the page (Main pops it).
Rectangle {
    id: root

    property string title
    default property alias content: body.data
    // Sheets and pages drawn over this one (each `anchors.fill: parent`).
    property alias overlays: overlayLayer.data
    readonly property alias scroll: scroll
    readonly property bool sheet: false
    // The predictive back gesture's progress (0..1): the page follows the
    // finger before the pop commits, as an Android activity does.
    property real backProgress: 0
    // Set before a confirm opens under the page's last key: the page
    // follows it down once, or its keys sat under the gesture bar.
    property bool revealEnd: false
    // Asked before the header's Back closes the page: true when it handled
    // Back itself (the hub clears a search first).
    property var beforeBack: null

    // The window's insets where this page meets its edges, read once here.
    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: Math.max(SafeArea.margins.bottom, Native.gestureInset)
    readonly property real keyboardPad: Math.max(bottomInset, Native.imeHeight)

    signal closeRequested()

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
        onBackRequested: {
            if (root.beforeBack && root.beforeBack())
                return
            root.closeRequested()
        }
    }

    Flickable {
        id: scroll
        anchors.top: header.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        contentHeight: body.height + root.keyboardPad
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        onContentHeightChanged: if (root.revealEnd) {
            root.revealEnd = false
            contentY = Math.max(0, contentHeight - height)
        }

        Column {
            id: body
            x: Math.max(16 + root.leftInset, (scroll.width - width) / 2)
            width: Math.min(scroll.width - 32 - root.leftInset - root.rightInset, 560)
            topPadding: 16
            bottomPadding: 32
            spacing: 32
        }
    }

    Item {
        id: overlayLayer
        anchors.fill: parent
        z: 2
    }
}
