pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import Yap.Chrome
import Yap.Media
import com.yap.native
import Yap.Ui

// The channel details' Media, Links and Files tabs (MemberList.tsx
// MediaTabContent, LinksTabContent, FilesTabContent), loaded afresh each
// time a tab shows (`/api/channels/{id}/media?type=`).
// - Media: a three-column grid of square tiles (radius 12, 6 apart), the
//   poster's 24 px avatar top right and the file name on hover; any tile
//   opens the in-app viewer with every item (a video plays inline there).
// - Links: ply-2 cards with the poster, the channel and when, then each
//   URL in the message as an accent link.
// - Files: ply-2 cards with the file glyph in a 44 px well, the name, size
//   and date, and who shared it; a card jumps to its message.
// While loading: a skeleton grid (Media) or a spinner; a failure says so
// with Try again; an empty tab shows its glyph and line.
Item {
    id: root

    required property string tab
    readonly property var view: JSON.parse(Extras.media)
    readonly property bool current: view.tab === tab && view.channelId === Nav.activeChannelId
    readonly property var rows: current ? view.rows : []
    readonly property bool loading: !current || (view.loading && rows.length === 0)

    signal jumpRequested(string messageId)

    function load() {
        Extras.loadMedia(root.tab)
    }

    Component.onCompleted: load()
    onTabChanged: load()

    // Every tile, pictures and videos alike, opens the in-app viewer at its
    // place in the list (MemberList.tsx handleMediaClick): a video plays
    // there, and the viewer's author chip, time and Jump to message come
    // from the row (messageId, author, avatar, createdMs).
    property bool viewerOpened: false
    function openMedia(index) {
        root.viewerOpened = true
        ShellState.imageViewerRequested(root.rows, index)
    }
    // The viewer's Jump to message closes the panel, as the web's does.
    Connections {
        target: ShellState
        function onViewerJumped(messageId) {
            if (root.viewerOpened)
                ShellState.showDetails = false
            root.viewerOpened = false
        }
    }

    function openLink(url) {
        if (Invites.openLink(url))
            return
        if (/^https?:/i.test(url))
            Files.openExternal(url)
    }

    // Media: the grid.
    GridView {
        id: grid
        visible: root.tab === "media" && !root.loading && root.rows.length > 0
        anchors.fill: parent
        cellWidth: Math.floor(width / 3)
        cellHeight: cellWidth
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        model: root.tab === "media" ? root.rows : []
        delegate: AbstractButton {
            id: tile
            required property var modelData
            required property int index
            readonly property bool webp: /\.webp($|[?#])/i.test(modelData.url)
            readonly property bool animated: !modelData.video && (modelData.gif === true || webp)
            width: grid.cellWidth
            height: grid.cellHeight
            hoverEnabled: true
            Accessible.role: Accessible.Button
            Accessible.name: modelData.name
            onClicked: root.openMedia(index)
            HoverHandler { cursorShape: Qt.PointingHandCursor }
            contentItem: Item {}
            // MediaGridImage: a picture, a video's poster, or (no poster) the
            // video's first frame as `<video preload="metadata">` shows it.
            // One that fails to load is the broken glyph on the tile
            // (`onError`), never a play key on a blank tile.
            readonly property bool peeks: modelData.video && (modelData.poster ?? "").length === 0
            property bool broken: false
            readonly property bool loaded: tile.animated
                || (tile.peeks ? (peekLoader.item !== null && peekLoader.item.pictureShown) : still.status === Image.Ready)
            onModelDataChanged: broken = false
            background: Item {
                Rectangle {
                    id: face
                    anchors.fill: parent
                    anchors.margins: 3
                    // rounded-xl is 2 px in the theme (styles.css), on
                    // ply-2 with a border-line/20 edge (primary/40 hovered).
                    radius: Theme.radius
                    color: Theme.ply2
                    clip: true
                    Image {
                        id: still
                        anchors.fill: parent
                        visible: !tile.animated && !tile.peeks && !tile.broken
                        source: tile.animated || tile.peeks ? "" : tile.modelData.video ? tile.modelData.poster : tile.modelData.url
                        sourceSize: Qt.size(240, 240)
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                        onStatusChanged: if (status === Image.Error) tile.broken = true
                    }
                    Loader {
                        id: peekLoader
                        anchors.fill: parent
                        active: tile.peeks && !tile.broken
                        sourceComponent: EmbedVideo {
                            source: tile.modelData.url
                            name: tile.modelData.name
                            bare: true
                            framed: false
                            preview: true
                            clipTo: grid
                            onFailed: tile.broken = true
                        }
                    }
                    // Loading: tint-1 with the spinner until the picture is in.
                    Rectangle {
                        anchors.fill: parent
                        visible: !tile.loaded && !tile.broken
                        color: Theme.tint1
                        Icon {
                            id: tileSpinner
                            anchors.centerIn: parent
                            name: "loader"
                            size: Theme.iconSm
                            color: Theme.ink3
                            RotationAnimator on rotation {
                                from: 0
                                to: 360
                                duration: Theme.durSpin
                                loops: Animation.Infinite
                                running: tileSpinner.visible && !Theme.reducedMotion
                            }
                        }
                    }
                    // Failed: the broken glyph (lg, ink-3 at 30 %) on ply-2.
                    Rectangle {
                        anchors.fill: parent
                        visible: tile.broken
                        color: Theme.ply2
                        Icon {
                            anchors.centerIn: parent
                            name: "imageBroken"
                            size: Theme.iconLg
                            color: Qt.rgba(Theme.ink3.r, Theme.ink3.g, Theme.ink3.b, 0.3)
                        }
                    }
                    // A GIF plays in its tile, as the web's <img> does, while
                    // on screen and the app is in front; an animated WebP
                    // through AnimatedPicture (Qt's WebP reader stops after
                    // one pass even when the file loops forever).
                    Loader {
                        anchors.fill: parent
                        anchors.margins: 1
                        active: tile.animated
                        sourceComponent: tile.webp ? webpTile : gifTile
                    }
                    Component {
                        id: webpTile
                        AnimatedPicture {
                            source: tile.modelData.url
                            fillMode: AnimatedPicture.PreserveAspectCrop
                            playing: true
                        }
                    }
                    Component {
                        id: gifTile
                        // Streamed, not AnimatedImage: Qt 6.11's AnimatedImage
                        // plays a network GIF from its reply, which cannot
                        // rewind, so it stopped after one pass. AnimatedPicture
                        // pauses off screen and behind other apps by itself.
                        AnimatedPicture {
                            source: tile.modelData.url
                            fillMode: AnimatedPicture.PreserveAspectCrop
                            stream: true
                            playing: true
                        }
                    }
                    // A video's play mark: a 40 px black/60 disc with a
                    // white/10 edge carrying the 20 px filled play in white.
                    Rectangle {
                        anchors.centerIn: parent
                        visible: tile.modelData.video && !tile.broken
                        width: 40
                        height: 40
                        radius: 20
                        color: Qt.rgba(0, 0, 0, 0.6)
                        border.width: 1
                        border.color: Qt.rgba(1, 1, 1, 0.1)
                        Icon {
                            anchors.centerIn: parent
                            anchors.horizontalCenterOffset: 1
                            name: "play"
                            variant: "active"
                            size: Theme.iconLg
                            color: "#ffffff"
                        }
                    }
                    // Hovered: black/60 to clear over the whole tile, the
                    // name in 11/600 white at its foot (p-2).
                    Rectangle {
                        anchors.fill: parent
                        visible: tile.hovered
                        gradient: Gradient {
                            GradientStop { position: 0; color: "transparent" }
                            GradientStop { position: 1; color: Qt.rgba(0, 0, 0, 0.6) }
                        }
                        Text {
                            x: 8
                            width: parent.width - 16
                            anchors.bottom: parent.bottom
                            anchors.bottomMargin: 8
                            elide: Text.ElideRight
                            textFormat: Text.PlainText
                            text: tile.modelData.name
                            color: "#ffffff"
                            font.family: "Yap Geist SemiBold"
                            font.pixelSize: 11
                            font.weight: Font.DemiBold
                        }
                    }
                    Avatar {
                        anchors.top: parent.top
                        anchors.right: parent.right
                        anchors.margins: 6
                        size: 24
                        initials: tile.modelData.initials
                        source: tile.modelData.avatar
                        border.width: 2
                        border.color: Qt.rgba(0, 0, 0, 0.3)
                    }
                }
                // The tile's edge over its picture: line/20, primary/40
                // while hovered.
                Rectangle {
                    anchors.fill: face
                    color: "transparent"
                    radius: face.radius
                    border.width: 1
                    border.color: tile.hovered
                        ? Qt.rgba(Theme.primaryFill.r, Theme.primaryFill.g, Theme.primaryFill.b, 0.4)
                        : Qt.rgba(Theme.line.r, Theme.line.g, Theme.line.b, Theme.line.a * 0.2)
                    EaseColor on border.color {}
                }
                Rectangle {
                    anchors.fill: face
                    visible: tile.visualFocus
                    color: "transparent"
                    radius: face.radius
                    border.width: 2
                    border.color: Theme.focus
                }
            }
        }
    }

    // Links and files: the cards.
    ListView {
        id: cards
        visible: root.tab !== "media" && !root.loading && root.rows.length > 0
        anchors.fill: parent
        spacing: root.tab === "links" ? 10 : 8
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        model: root.tab !== "media" ? root.rows : []
        delegate: Loader {
            id: card
            required property var modelData
            width: ListView.view.width
            sourceComponent: root.tab === "links" ? linkCard : fileCard
            Component {
                id: linkCard
                LinkCard { item: card.modelData }
            }
            Component {
                id: fileCard
                FileCard { item: card.modelData }
            }
        }
        ScrollBar.vertical: ThinScrollBar {}
    }

    // Loading: the media skeleton, else a spinner.
    Grid {
        visible: root.loading && root.tab === "media"
        width: parent.width
        columns: 3
        Repeater {
            model: 9
            Item {
                width: root.width / 3
                height: width
                Rectangle {
                    anchors.fill: parent
                    anchors.margins: 3
                    // MediaSkeletonGrid: rounded-lg (2 px), tint-1.
                    radius: Theme.radius
                    color: Theme.tint1
                }
            }
        }
    }
    Icon {
        id: spinner
        visible: root.loading && root.tab !== "media"
        anchors.horizontalCenter: parent.horizontalCenter
        y: 40
        name: "loader"
        size: Theme.iconLg
        color: Theme.ink3
        RotationAnimator on rotation {
            from: 0
            to: 360
            duration: Theme.durSpin
            loops: Animation.Infinite
            running: spinner.visible && !Theme.reducedMotion
        }
    }

    // Failed, or empty.
    Column {
        visible: !root.loading && root.rows.length === 0
        anchors.horizontalCenter: parent.horizontalCenter
        y: 40
        width: parent.width - 16
        spacing: 12
        Icon {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: root.view.error.length === 0
            name: root.tab === "media" ? "image" : root.tab === "links" ? "link" : "fileText"
            size: Theme.icon2xl
            color: Theme.ink3
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: root.view.error.length > 0 ? root.view.error
                : root.tab === "media" ? qsTr("No media shared yet")
                : root.tab === "links" ? qsTr("No links shared yet") : qsTr("No files shared yet")
            color: root.view.error.length > 0 ? Theme.dangerInk : Theme.ink2
            font: Theme.type.description
        }
        KitButton {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: root.view.error.length > 0
            size: "sm"
            iconName: "refresh"
            text: qsTr("Try again")
            onClicked: root.load()
        }
    }

    component LinkCard: Rectangle {
        id: link
        required property var item
        implicitHeight: linkBody.implicitHeight + 24
        radius: Theme.radiusSheet * 2
        color: linkHover.hovered ? Qt.tint(Theme.ply2, Theme.tint1) : Theme.ply2
        border.width: 1
        border.color: Theme.line
        HoverHandler { id: linkHover }
        Column {
            id: linkBody
            x: 12
            y: 12
            width: parent.width - 24
            spacing: 6
            Item {
                width: parent.width
                height: 20
                Avatar {
                    id: linkFace
                    size: 20
                    initials: link.item.initials
                    source: link.item.avatar
                }
                Text {
                    anchors.left: linkFace.right
                    anchors.leftMargin: 8
                    anchors.right: linkWhen.left
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                    text: link.item.author + "  ·  #" + Nav.activeChannelName
                    color: Theme.ink1
                    font: Theme.type.captionStrong
                }
                Text {
                    id: linkWhen
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: link.item.createdMs > 0 ? Extras.relativeTime(link.item.createdMs) : ""
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
            Repeater {
                model: link.item.urls
                AbstractButton {
                    id: url
                    HoverHandler { cursorShape: Qt.PointingHandCursor }
                    required property string modelData
                    width: linkBody.width
                    height: 22
                    hoverEnabled: true
                    Accessible.role: Accessible.Link
                    Accessible.name: modelData
                    onClicked: root.openLink(modelData)
                    contentItem: Row {
                        spacing: 8
                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: "externalLink"
                            size: Theme.iconXs
                            color: Theme.accentInk
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            width: url.width - 20
                            elide: Text.ElideRight
                            textFormat: Text.PlainText
                            text: url.modelData
                            color: Theme.accentInk
                            font.family: Theme.type.description.family
                            font.pixelSize: Theme.type.description.pixelSize
                            font.underline: url.hovered
                        }
                    }
                    background: Rectangle {
                        color: "transparent"
                        border.width: url.visualFocus ? 2 : 0
                        border.color: Theme.focus
                    }
                }
            }
        }
    }

    component FileCard: AbstractButton {
        id: file
        required property var item
        implicitHeight: 70
        hoverEnabled: true
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Jump to shared file %1").arg(item.name)
        onClicked: root.jumpRequested(item.messageId)
        background: Rectangle {
            radius: Theme.radiusSheet * 2
            color: file.hovered ? Qt.tint(Theme.ply2, Theme.tint1) : Theme.ply2
            EaseColor on color {}
            border.width: file.visualFocus ? 2 : 1
            border.color: file.visualFocus ? Theme.focus : Theme.line
        }
        contentItem: Item {
            Rectangle {
                id: well
                x: 12
                anchors.verticalCenter: parent.verticalCenter
                width: 44
                height: 44
                radius: 12
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
                anchors.left: well.right
                anchors.leftMargin: 12
                anchors.right: parent.right
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Text {
                    width: parent.width
                    elide: Text.ElideMiddle
                    textFormat: Text.PlainText
                    text: file.item.name
                    color: Theme.ink1
                    font: Theme.type.panelTitle
                }
                Text {
                    width: parent.width
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                    text: file.item.size + "  ·  " + file.item.date
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
        }
    }
}
