pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// TikTok and Instagram (LinkEmbed.tsx TikTokEmbed :1176, InstagramEmbed
// :3971): a compact 400 card. The header, the author (32 px avatar, name,
// verified mark), then the media on black at the post's aspect (portrait
// stops at 560 and pillarboxes): a carousel with chevrons and dots when
// there is more than one, videos playing in place through the API's media
// proxy, pictures opening the viewer. Then likes, comments and views, the
// caption (Instagram leads with the author), "View all N comments" and the
// sound chip. The card asks the server for fresh media when it shows; with
// nothing playable TikTok falls back to its own player (WebView2).
Rectangle {
    id: root

    required property var embed
    property int maxWidth: 640
    property Item clipTo: null

    signal openLink(string url)
    signal openImage(var images, int index)
    signal expandVideo(string source, int position)

    property int index: 0
    readonly property var media: embed.media ?? []
    readonly property var current: media.length > 0 ? media[Math.min(index, media.length - 1)] : null
    readonly property real stageAspect: current && current.w > 0 && current.h > 0 ? current.w / current.h : embed.aspect

    width: Math.min(maxWidth, 400)
    height: body.implicitHeight + 24
    radius: Theme.radius
    color: Theme.ply2
    border.width: 1
    border.color: Theme.line

    // Asks for fresh media once per post: when the card shows, and again when
    // a pooled row is reused for another one.
    property bool hydrated: false
    property string hydratedPage: ""
    function hydrate() {
        const page = embed.page
        if (hydrated && page === hydratedPage)
            return
        hydrated = true
        hydratedPage = page
        index = 0
        Embeds.hydrate(embed.provider.toLowerCase(), page)
    }
    Component.onCompleted: hydrate()
    onEmbedChanged: hydrate()

    function images() {
        return media.filter(m => m.type === "image").map(m => ({ url: m.src, name: "", size: "", kind: "image" }))
    }
    function openPicture(at) {
        const list = media.filter(m => m.type === "image")
        const i = list.indexOf(media[at])
        root.openImage(images(), Math.max(0, i))
    }

    Column {
        id: body
        x: 12
        y: 12
        width: parent.width - 24
        spacing: 8

        EmbedHeader {
            width: parent.width
            provider: root.embed.provider
            colour: root.embed.color
            timestamp: root.embed.timestamp ?? ""
            openTip: root.embed.open
            url: root.embed.url
            onOpenLink: url => root.openLink(url)
        }

        // SocialAuthorRow
        Row {
            visible: root.embed.authorName.length > 0
            width: parent.width
            spacing: 8
            Rectangle {
                width: 32
                height: 32
                radius: Theme.radius
                color: Theme.tint3
                clip: true
                Image {
                    anchors.fill: parent
                    source: root.embed.authorAvatar
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                    sourceSize: Qt.size(64, 64)
                }
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, parent.width - 32 - 8 - (verified.visible ? 24 : 0))
                textFormat: Text.PlainText
                text: root.embed.authorName
                elide: Text.ElideRight
                color: Theme.ink1
                font.family: Theme.type.panelTitle.family
                font.pixelSize: 13
                font.underline: authorHover.hovered && root.embed.authorUrl.length > 0
                HoverHandler { id: authorHover; cursorShape: root.embed.authorUrl.length > 0 ? Qt.PointingHandCursor : Qt.ArrowCursor }
                TapHandler { enabled: root.embed.authorUrl.length > 0; onTapped: root.openLink(root.embed.authorUrl) }
            }
            Icon {
                id: verified
                anchors.verticalCenter: parent.verticalCenter
                visible: root.embed.verified === true
                name: "verified"
                variant: "active"
                size: Theme.iconSm
                color: Theme.accentInk
            }
        }

        // The media: a carousel on black.
        Rectangle {
            id: stage
            visible: root.embed.renderable
            width: parent.width
            height: Math.min(root.embed.maxHeight, Math.round(width / Math.max(0.2, root.stageAspect)))
            radius: Theme.radiusSheet * 2
            color: "#000000"
            clip: true

            Loader {
                anchors.fill: parent
                active: root.current !== null && root.current.type === "video"
                sourceComponent: EmbedVideo {
                    source: root.current.src
                    poster: root.current.poster
                    aspect: root.stageAspect
                    durationHint: root.current.duration
                    animated: root.current.gif === true
                    framed: false
                    clipTo: root.clipTo
                    name: root.embed.title
                    onExpandRequested: (source, position) => root.expandVideo(source, position)
                }
            }
            Image {
                anchors.fill: parent
                visible: root.current !== null && root.current.type === "image"
                source: visible ? root.current.src : ""
                fillMode: Image.PreserveAspectFit
                asynchronous: true
                sourceSize: Qt.size(Math.ceil(width * 2), Math.ceil(height * 2))
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.openPicture(root.index)
                }
                Text {
                    anchors.centerIn: parent
                    visible: parent.status === Image.Error
                    textFormat: Text.PlainText
                    text: qsTr("Media unavailable")
                    color: "#b4b4ba"
                    font: Theme.type.captionStrong
                }
            }

            // CarouselControls: ply-3 chevrons and position dots.
            Rectangle {
                visible: root.media.length > 1 && root.index > 0
                x: 8
                anchors.verticalCenter: parent.verticalCenter
                width: 32
                height: 32
                radius: 16
                color: prevHover.hovered ? Theme.ply4 : Theme.ply3
                border.width: 1
                border.color: Theme.line
                Icon { anchors.centerIn: parent; name: "chevronLeft"; size: Theme.iconSm; color: Theme.ink1 }
                HoverHandler { id: prevHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: root.index = Math.max(0, root.index - 1) }
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Previous media")
            }
            Rectangle {
                visible: root.media.length > 1 && root.index < root.media.length - 1
                anchors.right: parent.right
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                width: 32
                height: 32
                radius: 16
                color: nextHover.hovered ? Theme.ply4 : Theme.ply3
                border.width: 1
                border.color: Theme.line
                Icon { anchors.centerIn: parent; name: "chevronRight"; size: Theme.iconSm; color: Theme.ink1 }
                HoverHandler { id: nextHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: root.index = Math.min(root.media.length - 1, root.index + 1) }
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Next media")
            }
            Row {
                visible: root.media.length > 1
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 8
                spacing: 6
                Repeater {
                    model: root.media.length
                    Rectangle {
                        required property int index
                        width: 6
                        height: 6
                        radius: 3
                        color: Theme.ink1
                        opacity: index === root.index ? 1 : 0.45
                        scale: index === root.index ? 1 : 0.8
                        HoverHandler { cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: root.index = parent.index }
                    }
                }
            }
            focus: false
            Keys.onLeftPressed: root.index = Math.max(0, root.index - 1)
            Keys.onRightPressed: root.index = Math.min(root.media.length - 1, root.index + 1)
        }

        // Nothing playable: the cover while it loads, then TikTok's player.
        Item {
            visible: !root.embed.renderable
            width: parent.width
            height: Math.min(root.embed.maxHeight, Math.round(width * 16 / 9))
            WebPlayer {
                anchors.fill: parent
                visible: root.embed.iframe.length > 0 && (root.embed.state === "iframe" || root.embed.state === "ready")
                url: root.embed.iframe
                link: root.embed.url
                poster: root.embed.cover
                label: root.embed.title
                clipTo: root.clipTo
            }
            Rectangle {
                anchors.fill: parent
                visible: !(root.embed.iframe.length > 0 && (root.embed.state === "iframe" || root.embed.state === "ready"))
                radius: Theme.radiusSheet * 2
                color: "#000000"
                clip: true
                Image {
                    anchors.fill: parent
                    source: root.embed.cover
                    fillMode: Image.PreserveAspectCrop
                    opacity: root.embed.state === "loading" ? 0.5 : 1
                    asynchronous: true
                    sourceSize: Qt.size(Math.ceil(width * 2), Math.ceil(height * 2))
                }
                PlayDisc {
                    anchors.centerIn: parent
                    visible: root.embed.state !== "loading"
                }
                Rectangle {
                    anchors.centerIn: parent
                    visible: root.embed.state === "loading"
                    width: 48
                    height: 48
                    radius: 24
                    color: Theme.ply3
                    border.width: 1
                    border.color: Theme.line
                    Icon { anchors.centerIn: parent; name: "loader"; size: Theme.iconLg; color: Theme.ink1 }
                    Accessible.role: Accessible.Indicator
                    Accessible.name: qsTr("Loading %1").arg(root.embed.provider)
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.openLink(root.embed.url)
                }
            }
        }

        // SocialMetrics
        Row {
            visible: root.embed.renderable && (root.embed.likes.length > 0 || root.embed.comments.length > 0 || root.embed.views.length > 0)
            spacing: 16
            Repeater {
                model: [
                    { icon: "like", value: root.embed.likes, tip: qsTr("Likes") },
                    { icon: "chat", value: root.embed.comments, tip: qsTr("Comments") },
                    { icon: "eye", value: root.embed.views, tip: qsTr("Views") }
                ].filter(m => m.value.length > 0)
                Row {
                    required property var modelData
                    spacing: 6
                    Accessible.role: Accessible.StaticText
                    Accessible.name: modelData.tip + " " + modelData.value
                    Icon { anchors.verticalCenter: parent.verticalCenter; name: parent.modelData.icon; size: Theme.iconSm; color: Theme.ink2 }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: parent.modelData.value
                        color: Theme.ink2
                        font: Theme.type.num
                    }
                }
            }
        }

        Text {
            visible: root.embed.renderable && root.embed.caption.length > 0
            width: parent.width
            textFormat: Text.PlainText
            text: (root.embed.captionAuthor.length > 0 ? root.embed.captionAuthor + " " : "") + root.embed.caption
            wrapMode: Text.Wrap
            color: Theme.ink1
            font: Theme.type.description
            lineHeight: 20
            lineHeightMode: Text.FixedHeight
        }

        Text {
            visible: root.embed.renderable && root.embed.commentsLink.length > 0
            textFormat: Text.PlainText
            text: root.embed.commentsLink
            color: commentsHover.hovered ? Theme.ink1 : Theme.ink3
            font: Theme.type.caption
            HoverHandler { id: commentsHover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: root.openLink(root.embed.url) }
        }

        SocialAudioChip {
            visible: root.embed.renderable && root.embed.audio !== null
            width: parent.width
            audio: root.embed.audio
            clipTo: root.clipTo
        }
    }
}
