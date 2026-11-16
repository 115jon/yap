pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The open conversation's Media, Links and Files, a page over the pager
// (the desktop's channel details tabs, MediaTabs.qml, phone layout), on the
// shared MediaTabsCore. The floating bar holds the back key and the
// conversation over "Media, links and files"; the Segmented rail under it
// picks the tab, each loaded afresh as it shows.
// - Media: a three-column grid of square tiles (pictures, a video's poster
//   under the play disc, a GIF's still with its mark); a tap opens the
//   viewer at that tile with every item (a video plays from there). A long
//   press offers View and Jump to message.
// - Links: a row per message, the poster, when, then each link in accent
//   ink; a tap opens the link (one of ours opens here), a long press
//   offers Open, Copy, Share and Jump to message.
// - Files: a row per file, its glyph in a 44 px well, the name, size and
//   date and who shared it; a tap shows the message in the chat (as the
//   desktop's card does), the download key saves it into Downloads/Yap.
// Loading shows a skeleton grid or rows, a failure says why with Try
// again, an empty tab says what shows there.
Rectangle {
    id: root

    readonly property bool sheet: false
    property real backProgress: 0

    signal closeRequested()
    signal conversationRequested(string channelId)

    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: Math.max(SafeArea.margins.bottom, Native.gestureInset)

    readonly property bool dm: Nav.dmUserId.length > 0
    readonly property var tabs: ["media", "links", "files"]

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

    MediaTabsCore {
        id: core
        tab: "media"
        follow: true
        // The chat under this page lands on it.
        onJumped: root.closeRequested()
    }

    // A link from a message: ours opens here (a channel slides the chat
    // in), a web link outside the app.
    function openLink(url) {
        const channelId = Invites.linkChannel(url)
        if (core.openOwn(url)) {
            // A channel slides the chat in; an invite raises its landing
            // over this page; a server or home link leaves it.
            if (channelId.length > 0)
                root.conversationRequested(channelId)
            else if (!/\/invite\//i.test(url))
                root.closeRequested()
            return
        }
        if (core.isWeb(url))
            Qt.openUrlExternally(url)
    }

    function openMedia(index) {
        Native.haptic(Native.Tick)
        viewer.show(core.viewerItems(), index)
    }

    // ---- Save a file into Downloads/Yap -------------------------------------

    // FileSaves runs the save (it outlives this page); the note says how
    // it ended while the page is up.
    function save(row) {
        if (FileSaves.save(row.id, row.url, row.name))
            Native.haptic(Native.Tick)
    }
    Connections {
        target: FileSaves
        function onFinished(key, ok, text) {
            if (ok)
                Native.haptic(Native.Confirm)
            root.flash(text)
        }
    }

    // A short line over the foot of the page.
    property string note: ""
    function flash(text) {
        root.note = text
        noteTimer.restart()
    }
    Timer {
        id: noteTimer
        interval: 2500
        onTriggered: root.note = ""
    }

    // ---- The bar and the rail -------------------------------------------------

    Item {
        id: bar
        x: 8 + root.leftInset
        y: root.topInset + 8
        width: parent.width - 16 - root.leftInset - root.rightInset
        height: Math.max(56, Math.ceil(kicker.implicitHeight + title.implicitHeight + 12))
        z: 3

        FloatPlate {
            anchors.fill: parent
        }
        IconKey {
            id: backKey
            anchors.left: parent.left
            anchors.leftMargin: 4
            anchors.verticalCenter: parent.verticalCenter
            iconName: "back"
            tip: qsTr("Back to the conversation")
            onClicked: root.closeRequested()
        }
        Column {
            anchors.left: backKey.right
            anchors.leftMargin: 4
            anchors.right: parent.right
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            Text {
                id: kicker
                width: parent.width
                textFormat: Text.PlainText
                text: root.dm ? Nav.activeChannelName : "#" + Nav.activeChannelName
                elide: Text.ElideRight
                color: Theme.ink3
                font: Theme.type.silk
                Accessible.ignored: true
            }
            Text {
                id: title
                width: parent.width
                textFormat: Text.PlainText
                text: qsTr("Media, links and files")
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.columnTitle
                Accessible.role: Accessible.Heading
                Accessible.name: root.dm ? qsTr("Media, links and files with %1").arg(Nav.activeChannelName)
                    : qsTr("Media, links and files in %1").arg(Nav.activeChannelName)
            }
        }
    }

    // A solid band under the status bar and the bar: the rows scroll
    // behind the rail, never behind the clock and the system's icons.
    Rectangle {
        width: parent.width
        height: rail.y + rail.height + 8
        z: 2
        color: Theme.ply0
        // Taps here never reach the rows scrolled under it.
        MouseArea {
            anchors.fill: parent
        }
    }

    Segmented {
        id: rail
        x: 16 + root.leftInset
        y: bar.y + bar.height + 12
        z: 3
        width: parent.width - 32 - root.leftInset - root.rightInset
        accessibleName: qsTr("Show")
        options: [qsTr("Media"), qsTr("Links"), qsTr("Files")]
        current: root.tabs.indexOf(core.tab)
        onPicked: index => core.tab = root.tabs[index]
    }

    readonly property real listTop: rail.y + rail.height + 12

    // ---- Media ------------------------------------------------------------------

    GridView {
        id: grid
        visible: core.tab === "media" && !core.loading && core.rows.length > 0
        anchors.fill: parent
        anchors.leftMargin: 14 + root.leftInset
        anchors.rightMargin: 14 + root.rightInset
        topMargin: root.listTop
        bottomMargin: root.bottomInset + 16
        cellWidth: Math.floor(width / 3)
        cellHeight: cellWidth
        boundsBehavior: Flickable.StopAtBounds
        model: core.tab === "media" ? core.rows : []
        Accessible.role: Accessible.List
        Accessible.name: qsTr("Media")
        delegate: Item {
            id: tile
            required property var modelData
            required property int index
            readonly property bool video: modelData.video === true
            readonly property bool animated: core.isAnimated(modelData)
            // A video without a poster: its first frame (the viewer's own
            // provider, cpp/videoplayer.h).
            readonly property string still: !video ? modelData.url
                : (modelData.poster ?? "").length > 0 ? modelData.poster
                : core.isWeb(modelData.url) ? "image://videoposter/" + encodeURIComponent(modelData.url) : ""
            width: grid.cellWidth
            height: grid.cellHeight
            Accessible.role: Accessible.Button
            Accessible.name: video ? qsTr("Play %1, from %2").arg(modelData.name).arg(modelData.author)
                : qsTr("Open %1, from %2").arg(modelData.name).arg(modelData.author)
            Accessible.onPressAction: root.openMedia(index)

            Rectangle {
                id: face
                anchors.fill: parent
                anchors.margins: 2
                radius: Theme.radius
                color: Theme.tint1
                border.width: 1
                border.color: Theme.line
                clip: true
                scale: tileTap.pressed ? Theme.scalePressIcon : 1
                Behavior on scale { NumberAnimation { duration: Theme.dur1 } }

                Image {
                    id: picture
                    anchors.fill: parent
                    anchors.margins: 1
                    visible: status === Image.Ready
                    source: tile.still
                    sourceSize: Qt.size(320, 320)
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                    Accessible.ignored: true
                }
                // Loading: the spinner on tint-1; failed: the broken glyph.
                Spinner {
                    anchors.centerIn: parent
                    size: Theme.iconSm
                    running: tile.still.length > 0 && picture.status === Image.Loading
                }
                Icon {
                    anchors.centerIn: parent
                    visible: picture.status === Image.Error
                    name: "imageBroken"
                    size: Theme.iconLg
                    color: Theme.ink3
                }
                PlayDisc {
                    anchors.centerIn: parent
                    visible: tile.video
                    scale: 0.75
                }
                // A GIF's mark (it plays in the viewer).
                Rectangle {
                    visible: tile.animated
                    anchors.left: parent.left
                    anchors.bottom: parent.bottom
                    anchors.margins: 6
                    width: gifWord.implicitWidth + 8
                    height: gifWord.implicitHeight + 2
                    radius: Theme.radius
                    color: Qt.rgba(0, 0, 0, 0.6)
                    Text {
                        id: gifWord
                        anchors.centerIn: parent
                        textFormat: Text.PlainText
                        text: qsTr("GIF")
                        color: "#ffffff"
                        font: Theme.type.silk
                    }
                }
                Avatar {
                    anchors.top: parent.top
                    anchors.right: parent.right
                    anchors.margins: 6
                    size: 22
                    initials: tile.modelData.initials
                    source: tile.modelData.avatar
                    Accessible.ignored: true
                }
            }
            TapHandler {
                id: tileTap
                gesturePolicy: TapHandler.ReleaseWithinBounds
                longPressThreshold: 0.4
                onTapped: root.openMedia(tile.index)
                onLongPressed: {
                    Native.haptic(Native.LongPress)
                    root.act("media", tile.modelData, "", tile.index)
                }
            }
        }
    }

    // ---- Links and files ------------------------------------------------------

    PlatformList {
        id: list
        visible: core.tab !== "media" && !core.loading && core.rows.length > 0
        anchors.fill: parent
        anchors.leftMargin: root.leftInset
        anchors.rightMargin: root.rightInset
        topMargin: root.listTop
        bottomMargin: root.bottomInset + 16
        model: core.tab !== "media" ? core.rows : []
        Accessible.role: Accessible.List
        Accessible.name: core.tab === "links" ? qsTr("Links") : qsTr("Files")
        delegate: Loader {
            id: entry
            required property var modelData
            width: ListView.view.width
            sourceComponent: core.tab === "links" ? linkRow : fileRow
            Component {
                id: linkRow
                LinkRow { item: entry.modelData }
            }
            Component {
                id: fileRow
                FileRow { item: entry.modelData }
            }
        }
    }

    // A message's links: the poster and when, then each link a row of its own.
    component LinkRow: Item {
        id: link
        required property var item
        implicitHeight: linkBody.y + linkBody.height + 8 + 1

        Avatar {
            id: linkFace
            x: 16
            y: 12
            size: 36
            initials: link.item.initials
            source: link.item.avatar
            Accessible.ignored: true
        }
        Column {
            id: linkBody
            x: 16 + 36 + 12
            y: 12
            width: link.width - x - 8
            Row {
                width: parent.width
                spacing: 8
                Text {
                    id: linkName
                    width: Math.min(implicitWidth, parent.width - linkWhen.implicitWidth - 16)
                    textFormat: Text.PlainText
                    text: link.item.author
                    elide: Text.ElideRight
                    color: Theme.ink1
                    font: Theme.type.author
                }
                Text {
                    id: linkWhen
                    anchors.baseline: linkName.baseline
                    textFormat: Text.PlainText
                    text: link.item.createdMs > 0 ? Extras.relativeTime(link.item.createdMs) : ""
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
            Repeater {
                model: link.item.urls
                delegate: Item {
                    id: url
                    required property string modelData
                    width: linkBody.width
                    height: Math.max(Theme.hitMin, urlText.implicitHeight + 12)
                    Accessible.role: Accessible.Link
                    Accessible.name: modelData
                    Accessible.onPressAction: root.openLink(modelData)
                    Rectangle {
                        anchors.fill: parent
                        anchors.leftMargin: -8
                        radius: Theme.radius
                        color: urlTap.pressed ? Theme.tint2 : "transparent"
                        EaseColor on color {}
                    }
                    Icon {
                        id: urlGlyph
                        anchors.verticalCenter: parent.verticalCenter
                        name: "externalLink"
                        size: Theme.iconSm
                        color: Theme.accentInk
                    }
                    Text {
                        id: urlText
                        anchors.left: urlGlyph.right
                        anchors.leftMargin: 8
                        anchors.right: parent.right
                        anchors.rightMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: url.modelData
                        elide: Text.ElideRight
                        color: Theme.accentInk
                        font: Theme.type.description
                    }
                    TapHandler {
                        id: urlTap
                        gesturePolicy: TapHandler.ReleaseWithinBounds
                        longPressThreshold: 0.4
                        onTapped: {
                            Native.haptic(Native.Tick)
                            root.openLink(url.modelData)
                        }
                        onLongPressed: {
                            Native.haptic(Native.LongPress)
                            root.act("link", link.item, url.modelData, -1)
                        }
                    }
                }
            }
        }
        Rectangle {
            anchors.bottom: parent.bottom
            x: 16
            width: parent.width - 16
            height: 1
            color: Theme.line
        }
    }

    // A file: its glyph, name, size and date, who shared it, and Save.
    component FileRow: Item {
        id: file
        required property var item
        readonly property bool saving: FileSaves.saving[item.id] !== undefined
        implicitHeight: Math.max(76, fileText.implicitHeight + 24) + 1
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Jump to shared file %1").arg(item.name)
        Accessible.onPressAction: core.jump(item.messageId)

        Rectangle {
            anchors.fill: parent
            color: fileTap.pressed ? Theme.tint1 : "transparent"
            EaseColor on color {}
        }
        TapHandler {
            id: fileTap
            gesturePolicy: TapHandler.ReleaseWithinBounds
            longPressThreshold: 0.4
            onTapped: {
                Native.haptic(Native.Tick)
                core.jump(file.item.messageId)
            }
            onLongPressed: {
                Native.haptic(Native.LongPress)
                root.act("file", file.item, "", -1)
            }
        }
        Rectangle {
            id: well
            x: 16
            anchors.verticalCenter: parent.verticalCenter
            width: 44
            height: 44
            radius: Theme.radius
            color: Theme.ply2
            border.width: 1
            border.color: Theme.line
            Icon {
                anchors.centerIn: parent
                name: file.item.icon
                size: Theme.iconXl
                color: Theme.ink2
            }
        }
        Column {
            id: fileText
            anchors.left: well.right
            anchors.leftMargin: 12
            anchors.right: saveKey.left
            anchors.rightMargin: 4
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: file.item.name
                elide: Text.ElideMiddle
                color: Theme.ink1
                font: Theme.type.author
            }
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: file.item.size + "  ·  " + file.item.date
                elide: Text.ElideRight
                color: Theme.ink3
                font: Theme.type.caption
            }
            Row {
                spacing: 6
                Avatar {
                    anchors.verticalCenter: parent.verticalCenter
                    size: 16
                    initials: file.item.initials
                    source: file.item.avatar
                    Accessible.ignored: true
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: file.item.author
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
        }
        IconKey {
            id: saveKey
            anchors.right: parent.right
            anchors.rightMargin: 4
            anchors.verticalCenter: parent.verticalCenter
            visible: !file.saving
            iconName: "download"
            tone: "ghost"
            tip: qsTr("Save to Downloads")
            Accessible.name: qsTr("Save %1 to Downloads").arg(file.item.name)
            onClicked: root.save(file.item)
        }
        Spinner {
            anchors.centerIn: saveKey
            size: Theme.iconSm
            running: file.saving
        }
        Rectangle {
            anchors.bottom: parent.bottom
            x: 16
            width: parent.width - 16
            height: 1
            color: Theme.line
        }
    }

    // ---- Loading, failed, empty ---------------------------------------------------

    // Media: the skeleton grid.
    Grid {
        visible: core.loading && core.tab === "media"
        x: 14 + root.leftInset
        y: root.listTop
        columns: 3
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("Loading")
        Repeater {
            model: 9
            delegate: Item {
                width: Math.floor((root.width - 28 - root.leftInset - root.rightInset) / 3)
                height: width
                Rectangle {
                    anchors.fill: parent
                    anchors.margins: 2
                    radius: Theme.radius
                    color: Theme.tint2
                }
            }
        }
    }
    // Links and files: the row's anatomy.
    Column {
        visible: core.loading && core.tab !== "media"
        x: root.leftInset
        y: root.listTop
        width: parent.width - root.leftInset - root.rightInset
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("Loading")
        Repeater {
            model: 4
            delegate: Item {
                width: parent.width
                height: 77
                Rectangle {
                    x: 16
                    y: 16
                    width: core.tab === "files" ? 44 : 36
                    height: width
                    radius: Theme.radius
                    color: Theme.tint2
                }
                Column {
                    x: 16 + 44 + 12
                    y: 20
                    width: parent.width - x - 16
                    spacing: 8
                    Rectangle { width: 112; height: 12; radius: Theme.radius; color: Theme.tint2 }
                    Rectangle { width: parent.width * 2 / 3; height: 12; radius: Theme.radius; color: Theme.tint2 }
                }
                Rectangle {
                    anchors.bottom: parent.bottom
                    x: 16
                    width: parent.width - 16
                    height: 1
                    color: Theme.line
                }
            }
        }
    }

    // A failure: why, and Try again.
    Column {
        visible: core.failed
        x: 32 + root.leftInset
        y: root.listTop + 48
        width: parent.width - 64 - root.leftInset - root.rightInset
        spacing: 12
        Accessible.role: Accessible.AlertMessage
        Accessible.name: core.error
        Icon {
            anchors.horizontalCenter: parent.horizontalCenter
            name: "offline"
            size: Theme.icon2xl
            color: Theme.ink3
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            textFormat: Text.PlainText
            text: core.tab === "media" ? qsTr("Couldn't load the media")
                : core.tab === "links" ? qsTr("Couldn't load the links") : qsTr("Couldn't load the files")
            color: Theme.ink1
            font: Theme.type.sectionTitle
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            textFormat: Text.PlainText
            text: core.error
            wrapMode: Text.Wrap
            color: Theme.ink2
            font: Theme.type.description
        }
        KitButton {
            anchors.horizontalCenter: parent.horizontalCenter
            variant: "secondary"
            iconName: "refresh"
            text: qsTr("Try again")
            onClicked: core.load()
        }
    }

    // None: the tab's glyph and line, and how things land here.
    Column {
        visible: core.empty
        x: 32 + root.leftInset
        y: root.listTop + 48
        width: parent.width - 64 - root.leftInset - root.rightInset
        spacing: 8
        Accessible.role: Accessible.StaticText
        Accessible.name: core.emptyText
        Icon {
            anchors.horizontalCenter: parent.horizontalCenter
            name: core.emptyIcon
            size: Theme.icon2xl
            color: Theme.ink3
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            textFormat: Text.PlainText
            text: core.emptyText
            color: Theme.ink1
            font: Theme.type.sectionTitle
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            textFormat: Text.PlainText
            text: core.tab === "media" ? qsTr("Pictures and videos posted here show up in this grid.")
                : core.tab === "links" ? qsTr("Links posted in messages here show up in this list.")
                : qsTr("Files posted here show up in this list.")
            wrapMode: Text.Wrap
            color: Theme.ink2
            font: Theme.type.description
        }
    }

    // The note (a save's outcome), over the foot of the page.
    Rectangle {
        visible: root.note.length > 0
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: root.bottomInset + 24
        z: 4
        width: Math.min(parent.width - 48, noteText.implicitWidth + 32)
        height: noteText.implicitHeight + 20
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.line
        Accessible.role: Accessible.AlertMessage
        Accessible.name: root.note
        Text {
            id: noteText
            anchors.centerIn: parent
            width: Math.min(implicitWidth, parent.parent.width - 80)
            textFormat: Text.PlainText
            text: root.note
            wrapMode: Text.Wrap
            horizontalAlignment: Text.AlignHCenter
            color: Theme.ink1
            font: Theme.type.description
        }
    }

    // ---- A long press's sheet ---------------------------------------------------

    // What the sheet acts on: "media", "link" or "file", its row, the link
    // pressed (links) and the tile's place (media).
    property string actingKind: ""
    property var acting: null
    property string actingUrl: ""
    property int actingIndex: -1
    function act(kind, row, url, index) {
        root.actingKind = kind
        root.acting = row
        root.actingUrl = url
        root.actingIndex = index
        actions.open()
    }

    BottomSheet {
        id: actions
        anchors.fill: parent
        z: 6
        title: root.actingKind === "link" ? root.actingUrl : root.acting ? root.acting.name : ""
        contentHeight: actionColumn.implicitHeight
        onClosed: root.acting = null

        Column {
            id: actionColumn
            width: parent.width
            bottomPadding: 8
            SheetRow {
                visible: root.actingKind === "media"
                text: root.acting?.video === true ? qsTr("Play") : qsTr("View")
                iconName: root.acting?.video === true ? "play" : "image"
                onTriggered: {
                    const index = root.actingIndex
                    actions.close()
                    root.openMedia(index)
                }
            }
            SheetRow {
                visible: root.actingKind === "link"
                text: qsTr("Open link")
                iconName: "externalLink"
                onTriggered: {
                    const url = root.actingUrl
                    actions.close()
                    root.openLink(url)
                }
            }
            SheetRow {
                visible: root.actingKind === "link"
                text: qsTr("Copy link")
                iconName: "copy"
                onTriggered: {
                    const url = root.actingUrl
                    actions.close()
                    Native.copy(url)
                    Native.haptic(Native.Confirm)
                    root.flash(qsTr("Link copied"))
                }
            }
            SheetRow {
                visible: root.actingKind === "link"
                text: qsTr("Share link")
                iconName: "share"
                onTriggered: {
                    const url = root.actingUrl
                    actions.close()
                    Native.share(url)
                }
            }
            SheetRow {
                visible: root.actingKind === "file"
                text: qsTr("Save to Downloads")
                iconName: "download"
                onTriggered: {
                    const row = root.acting
                    actions.close()
                    root.save(row)
                }
            }
            SheetRow {
                text: qsTr("Jump to message")
                iconName: "arrowRight"
                onTriggered: {
                    const id = root.acting ? root.acting.messageId : ""
                    actions.close()
                    core.jump(id)
                }
            }
        }
    }

    // ---- The viewer -------------------------------------------------------------

    // A tile's pictures and videos, over the page. Back closes it first.
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
