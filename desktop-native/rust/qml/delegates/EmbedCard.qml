import QtQuick
import com.yap.native

// One link preview (LinkEmbed.tsx), from the row's `embeds` role:
//   card    ply-2 with a seam, 12 px in, 8 px between: the header (6 px
//           dot in the embed's colour, the site, the open key), the title
//           (14/600 accent ink), up to three lines of description, then
//           the picture: beside it at 80 x 80 ("thumbnail", 400 wide) or
//           under it at most 300 high ("large", 520 wide). Players
//           (YouTube, video, rich) show their thumbnail with the play
//           disc; it opens the link (no web views here).
//   image   bare media inside 640 x 400 (direct links): opens the viewer.
//   video   a direct video file: the play disc opens it externally.
//   status  "Unavailable": the deleted or private post card.
Item {
    id: root

    required property var embed
    property int maxWidth: 640

    signal openLink(string url)
    signal openImage(var images, int index)

    readonly property string kind: embed.kind
    implicitWidth: loader.item ? loader.item.width : 0
    implicitHeight: loader.item ? loader.item.height : 0

    Loader {
        id: loader
        sourceComponent: root.kind === "image" ? bare
            : root.kind === "video" ? video
            : root.kind === "status" ? status : card
    }

    // The header row: dot, site, and the open key.
    component Header: Item {
        id: head
        property string provider
        property string colour
        property string openTip
        property string url
        width: parent ? parent.width : 0
        height: 20
        visible: provider.length > 0 || url.length > 0
        Rectangle {
            id: dot
            anchors.verticalCenter: parent.verticalCenter
            width: 6
            height: 6
            radius: 3
            color: head.colour.length > 0 ? head.colour : Theme.lineStrong
        }
        Text {
            anchors.left: dot.right
            anchors.leftMargin: 8
            anchors.right: openKey.left
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: head.provider
            elide: Text.ElideRight
            color: Theme.ink2
            font: Theme.type.captionStrong
        }
        IconKey {
            id: openKey
            anchors.right: parent.right
            anchors.rightMargin: -4
            anchors.verticalCenter: parent.verticalCenter
            size: 28
            tone: "muted"
            iconName: "externalLink"
            tip: head.openTip
            onClicked: root.openLink(head.url)
        }
    }

    Component {
        id: card
        Rectangle {
            id: frame
            readonly property bool thumb: root.embed.layout === "thumbnail"
            readonly property bool large: root.embed.layout === "large"
            width: Math.min(root.maxWidth, root.embed.width)
            height: body.implicitHeight + 24
            radius: Theme.radius
            color: Theme.ply2
            border.width: 1
            border.color: Theme.line

            Column {
                id: body
                x: 12
                y: 12
                width: frame.width - 24
                spacing: 8

                Row {
                    width: parent.width
                    spacing: 12
                    Column {
                        id: textCol
                        width: parent.width - (frame.thumb ? 80 + 12 : 0)
                        spacing: 8
                        Header {
                            provider: root.embed.provider
                            colour: root.embed.color
                            openTip: root.embed.open
                            url: root.embed.url
                        }
                        Text {
                            visible: root.embed.author.length > 0
                            width: parent.width
                            textFormat: Text.PlainText
                            text: root.embed.author
                            elide: Text.ElideRight
                            color: Theme.ink1
                            font: Theme.type.captionStrong
                        }
                        Text {
                            id: title
                            visible: text.length > 0
                            width: parent.width
                            textFormat: Text.PlainText
                            text: root.embed.title
                            wrapMode: Text.Wrap
                            maximumLineCount: 2
                            elide: Text.ElideRight
                            color: Theme.accentInk
                            font.family: Theme.type.panelTitle.family
                            font.pixelSize: 14
                            font.underline: titleHover.containsMouse
                            lineHeight: 20
                            lineHeightMode: Text.FixedHeight
                            Accessible.role: Accessible.Link
                            Accessible.name: text
                            MouseArea {
                                id: titleHover
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: root.openLink(root.embed.url)
                            }
                        }
                        Text {
                            visible: text.length > 0
                            width: parent.width
                            text: root.embed.description
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                            maximumLineCount: 3
                            elide: Text.ElideRight
                            color: Theme.ink2
                            font: Theme.type.description
                            lineHeight: 20
                            lineHeightMode: Text.FixedHeight
                        }
                    }
                    // "thumbnail": 80 x 80 beside the text.
                    Rectangle {
                        visible: frame.thumb
                        width: 80
                        height: 80
                        radius: Theme.radius
                        color: Theme.tint1
                        clip: true
                        Image {
                            anchors.fill: parent
                            asynchronous: true
                            fillMode: Image.PreserveAspectCrop
                            sourceSize: Qt.size(160, 160)
                            source: frame.thumb ? root.embed.image : ""
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.openLink(root.embed.url)
                        }
                    }
                }

                // "large": full width under the text, at most 300 high
                // (1.91:1 when the size is unknown); players keep their box.
                Rectangle {
                    id: picture
                    readonly property real aspect: root.embed.w > 0 && root.embed.h > 0
                        ? root.embed.h / root.embed.w : 1 / 1.91
                    visible: frame.large
                    width: parent.width
                    height: Math.min(root.embed.play ? 560 : 300, Math.round(width * aspect))
                    radius: Theme.radius
                    color: root.embed.play ? "#000000" : Theme.tint1
                    clip: true
                    Image {
                        id: largeImage
                        anchors.fill: parent
                        asynchronous: true
                        fillMode: Image.PreserveAspectCrop
                        sourceSize: Qt.size(Math.ceil(width), Math.ceil(height))
                        source: frame.large ? root.embed.image : ""
                    }
                    PlayDisc {
                        anchors.centerIn: parent
                        visible: root.embed.play
                        hovered: pictureHover.containsMouse
                    }
                    MouseArea {
                        id: pictureHover
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.embed.play
                            ? root.openLink(root.embed.url)
                            : root.openImage([{ url: root.embed.image, name: root.embed.title, size: "", kind: "image" }], 0)
                    }
                    Accessible.role: Accessible.Button
                    Accessible.name: root.embed.play
                        ? (root.embed.title.length > 0 ? qsTr("Play %1").arg(root.embed.title) : qsTr("Play video"))
                        : qsTr("Open image")
                }

                Text {
                    visible: text.length > 0
                    width: parent.width
                    textFormat: Text.PlainText
                    text: root.embed.footer
                    elide: Text.ElideRight
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
        }
    }

    // Direct media: the picture itself, fitted into 640 x 400.
    Component {
        id: bare
        MediaTile {
            readonly property real natW: root.embed.w
            readonly property real natH: root.embed.h
            readonly property var known: ShellState.mediaSize(root.embed.url)
            readonly property real w0: natW > 0 ? natW : known ? known.w : 0
            readonly property real h0: natH > 0 ? natH : known ? known.h : 0
            readonly property real fit: w0 > 0 ? Math.min(1, Math.min(root.maxWidth, 640) / w0, 400 / h0) : 1
            item: ({ id: root.embed.url, url: root.embed.url, name: "", gif: root.embed.gif, nsfw: false })
            fill: false
            width: w0 > 0 ? Math.max(48, Math.round(w0 * fit)) : Math.min(root.maxWidth, 320)
            height: w0 > 0 ? Math.max(48, Math.round(h0 * fit)) : 200
            Accessible.name: root.embed.gif ? qsTr("Open GIF") : qsTr("Open image")
            onActivated: root.openImage([{ url: root.embed.url, name: "", size: "", kind: "image", gif: root.embed.gif }], 0)
        }
    }

    Component {
        id: video
        Rectangle {
            width: Math.min(root.maxWidth, 400)
            height: Math.round(width * 9 / 16)
            radius: Theme.radius
            color: "#000000"
            border.width: 1
            border.color: Theme.line
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Play %1").arg(root.embed.name)
            PlayDisc {
                anchors.centerIn: parent
                hovered: directHover.containsMouse
            }
            Text {
                x: 12
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 10
                width: parent.width - 24
                textFormat: Text.PlainText
                text: root.embed.host
                elide: Text.ElideRight
                color: "#b4b4ba"
                font: Theme.type.caption
            }
            MouseArea {
                id: directHover
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.openLink(root.embed.url)
            }
        }
    }

    Component {
        id: status
        Rectangle {
            width: Math.min(root.maxWidth, 400)
            height: statusBody.implicitHeight + 24
            radius: Theme.radius
            color: Theme.ply2
            border.width: 1
            border.color: Theme.line
            Column {
                id: statusBody
                x: 12
                y: 12
                width: parent.width - 24
                spacing: 8
                Header {
                    provider: root.embed.provider
                    colour: ""
                    openTip: root.embed.open
                    url: root.embed.url
                }
                Row {
                    spacing: 12
                    width: parent.width
                    Rectangle {
                        width: 40
                        height: 40
                        radius: Theme.radius
                        color: Theme.tint1
                        Icon {
                            anchors.centerIn: parent
                            name: "linkBroken"
                            size: Theme.iconLg
                            color: Theme.ink3
                        }
                    }
                    Column {
                        width: parent.width - 52
                        spacing: 2
                        Text {
                            textFormat: Text.PlainText
                            text: root.embed.label
                            color: Theme.ink1
                            font: Theme.type.silk
                        }
                        Text {
                            width: parent.width
                            textFormat: Text.PlainText
                            text: root.embed.text
                            wrapMode: Text.Wrap
                            color: Theme.ink2
                            font: Theme.type.description
                        }
                        Text {
                            width: parent.width
                            textFormat: Text.PlainText
                            text: root.embed.short
                            elide: Text.ElideRight
                            color: Theme.ink3
                            font: Theme.type.caption
                        }
                    }
                }
            }
        }
    }
}
