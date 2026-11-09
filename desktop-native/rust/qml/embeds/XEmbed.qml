pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// An X post (LinkEmbed.tsx XEmbed :2223): a 520 card. The header, the
// author (32 px avatar, name, handle), the text (over five lines or 280
// characters it folds with Show more), the media grid, "From <domain>" for
// a link card, the quoted post as a ruled block (its text folds at four
// lines behind "..."), then replies, reposts, likes and views over a rule.
Rectangle {
    id: root

    required property var embed
    property int maxWidth: 640
    property Item clipTo: null

    signal openLink(string url)
    signal openImage(var images, int index)
    signal expandVideo(string source, int position)

    property bool expanded: false

    width: Math.min(maxWidth, 520)
    height: body.implicitHeight + 24
    radius: Theme.radius
    color: Theme.ply2
    border.width: 1
    border.color: Theme.line

    component Author: Row {
        id: author
        required property var who
        property int face: 32
        property bool linked: false
        spacing: 8
        Rectangle {
            width: author.face
            height: author.face
            radius: Theme.radius
            color: Theme.tint3
            clip: true
            visible: author.who.avatar.length > 0
            Image {
                anchors.fill: parent
                source: author.who.avatar
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                sourceSize: Qt.size(author.face * 2, author.face * 2)
            }
        }
        Text {
            id: nameText
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: author.who.name
            elide: Text.ElideRight
            color: Theme.ink1
            font.family: Theme.type.panelTitle.family
            font.pixelSize: 13
            font.underline: author.linked && nameHover.hovered
            HoverHandler { id: nameHover; cursorShape: author.linked ? Qt.PointingHandCursor : Qt.ArrowCursor }
            TapHandler { enabled: author.linked && (author.who.url ?? "").length > 0; onTapped: root.openLink(author.who.url) }
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            visible: text.length > 0
            textFormat: Text.PlainText
            text: author.who.handle
            elide: Text.ElideRight
            color: Theme.ink3
            font: Theme.type.description
        }
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

        Author {
            visible: root.embed.author !== null
            who: root.embed.author ?? ({ name: "", handle: "", avatar: "", url: "" })
            linked: true
        }

        Text {
            id: postText
            visible: text.length > 0
            width: parent.width
            textFormat: Text.PlainText
            text: root.embed.text
            wrapMode: Text.Wrap
            maximumLineCount: root.embed.collapse && !root.expanded ? 5 : 100000
            elide: Text.ElideRight
            color: Theme.ink1
            font: Theme.type.description
            lineHeight: 20
            lineHeightMode: Text.FixedHeight
        }
        Text {
            visible: root.embed.collapse === true
            textFormat: Text.PlainText
            text: root.expanded ? qsTr("Show less") : qsTr("Show more")
            color: Theme.accentInk
            font.family: Theme.type.captionStrong.family
            font.pixelSize: Theme.type.captionStrong.pixelSize
            font.weight: Theme.type.captionStrong.weight
            font.underline: moreHover.hovered
            HoverHandler { id: moreHover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: root.expanded = !root.expanded }
            Accessible.role: Accessible.Button
            Accessible.name: root.expanded ? qsTr("Collapse post text") : qsTr("Expand post text")
        }

        XMediaGrid {
            visible: root.embed.media.length > 0
            media: root.embed.media
            extra: root.embed.extra
            maxWidth: parent.width
            clipTo: root.clipTo
            chip: root.embed.chip
            onOpenImage: (images, index) => root.openImage(images, index)
            onExpandVideo: (source, position) => root.expandVideo(source, position)
        }

        Text {
            visible: root.embed.cardDomain.length > 0
            textFormat: Text.PlainText
            text: qsTr("From %1").arg(root.embed.cardDomain)
            color: fromHover.hovered ? Theme.ink1 : Theme.ink3
            EaseColor on color {}
            font: Theme.type.caption
            HoverHandler { id: fromHover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: root.openLink(root.embed.cardUrl) }
        }

        // XReferencedTweetCard: a ruled block, not a second card.
        Rectangle {
            id: quote
            visible: root.embed.quote !== null
            width: parent.width
            height: visible ? quoteBody.implicitHeight + 24 : 0
            radius: Theme.radiusSheet * 2
            color: "transparent"
            border.width: 1
            border.color: Theme.line
            property bool open: false
            readonly property var q: root.embed.quote ?? ({ author: null, text: "", collapse: false, url: "", media: [], extra: 0 })
            Column {
                id: quoteBody
                x: 12
                y: 12
                width: parent.width - 24
                spacing: 8
                Item {
                    visible: quote.q.author !== null
                    width: parent.width
                    height: 24
                    Author {
                        anchors.verticalCenter: parent.verticalCenter
                        who: quote.q.author ?? ({ name: "", handle: "", avatar: "" })
                        face: 20
                    }
                    IconKey {
                        anchors.right: parent.right
                        anchors.rightMargin: -4
                        anchors.verticalCenter: parent.verticalCenter
                        visible: quote.q.url.length > 0
                        size: 28
                        tone: "muted"
                        iconName: "externalLink"
                        tip: qsTr("Open quoted post on X")
                        onClicked: root.openLink(quote.q.url)
                    }
                }
                Text {
                    visible: text.length > 0
                    width: parent.width
                    textFormat: Text.PlainText
                    text: quote.q.text
                    wrapMode: Text.Wrap
                    maximumLineCount: quote.q.collapse && !quote.open ? 4 : 100000
                    elide: Text.ElideRight
                    color: Theme.ink1
                    font: Theme.type.description
                    lineHeight: 20
                    lineHeightMode: Text.FixedHeight
                }
                Text {
                    visible: quote.q.collapse === true
                    textFormat: Text.PlainText
                    text: quote.open ? qsTr("Show less") : "..."
                    color: quoteMore.hovered ? Theme.ink1 : Theme.ink3
                    EaseColor on color {}
                    font: Theme.type.captionStrong
                    HoverHandler { id: quoteMore; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: quote.open = !quote.open }
                }
                XMediaGrid {
                    visible: quote.q.media.length > 0
                    media: quote.q.media
                    extra: quote.q.extra
                    compact: root.embed.media.length > 0
                    maxWidth: parent.width
                    clipTo: root.clipTo
                    onOpenImage: (images, index) => root.openImage(images, index)
                    onExpandVideo: (source, position) => root.expandVideo(source, position)
                }
            }
        }

        // XMetricBar
        Column {
            visible: root.embed.metrics.length > 0
            width: parent.width
            spacing: 8
            Rectangle {
                width: parent.width
                height: 1
                color: Theme.line
            }
            Flow {
                width: parent.width
                spacing: 16
                Repeater {
                    model: root.embed.metrics
                    Row {
                        id: metric
                        required property var modelData
                        spacing: 6
                        Accessible.role: Accessible.StaticText
                        Accessible.name: modelData.label + ": " + modelData.raw
                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: metric.modelData.key === "replies" ? "reply"
                                : metric.modelData.key === "retweets" ? "repost"
                                : metric.modelData.key === "likes" ? "like" : "stats"
                            size: Theme.iconSm
                            color: Theme.ink3
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            visible: text.length > 0
                            textFormat: Text.PlainText
                            text: metric.modelData.value
                            color: Theme.ink3
                            font: Theme.type.num
                        }
                        HoverHandler { id: metricHover }
                        Tooltip {
                            visible: metricHover.hovered
                            text: metric.modelData.label + ": " + metric.modelData.raw
                        }
                    }
                }
            }
        }
    }
}
