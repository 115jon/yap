import QtQuick
import com.yap.native

// One picture of a message (ImageGrid.tsx tile): a 1 px seam, a tint-1
// ground while it loads, cover (`fill`) or contain, GIFs animated while on
// screen. A sensitive one stays blurred behind "Show media" until clicked
// (SensitiveMediaFrame.tsx); the blur is a tiny decode scaled up, so no
// shader runs per row. `more` puts the "+N" scrim on the last grid tile.
Rectangle {
    id: root

    required property var item
    property bool fill: true
    property int more: 0

    signal activated()

    readonly property bool hidden: item.nsfw && !ShellState.isRevealed(item.id)
    readonly property bool animated: item.gif && !hidden
    readonly property Item picture: animated ? gifLoader.item : still

    radius: Theme.radius
    color: Theme.tint1
    border.width: 1
    border.color: Theme.line
    clip: true
    Accessible.role: Accessible.Button
    Accessible.name: item.name.length > 0 ? qsTr("Open %1").arg(item.name) : qsTr("Open image")

    function learn(img) {
        // Only a decode at its own size tells the real size (a grid tile
        // decodes at the tile's).
        if (img && img.status === Image.Ready && !hidden && !fill)
            ShellState.rememberSize(item.url, img.implicitWidth, img.implicitHeight)
    }

    Image {
        id: still
        visible: !root.animated
        anchors.fill: parent
        anchors.margins: 1
        asynchronous: true
        cache: true
        smooth: true
        mipmap: !root.hidden
        fillMode: root.fill || root.hidden ? Image.PreserveAspectCrop : Image.PreserveAspectFit
        source: root.animated ? "" : root.item.url
        // A sensitive picture decodes at 16 px: scaled up, that is the blur.
        // A fitted one decodes at its own size (a cap would scale small
        // pictures up, and the learnt size must be the real one).
        sourceSize: root.hidden ? Qt.size(16, 16)
            : root.fill ? Qt.size(Math.ceil(width), Math.ceil(height)) : Qt.size(0, 0)
        onStatusChanged: root.learn(still)
    }

    Loader {
        id: gifLoader
        active: root.animated
        anchors.fill: parent
        anchors.margins: 1
        sourceComponent: AnimatedImage {
            id: anim
            asynchronous: true
            cache: true
            source: root.item.url
            fillMode: root.fill ? Image.PreserveAspectCrop : Image.PreserveAspectFit
            playing: root.visible && status === AnimatedImage.Ready
            onStatusChanged: root.learn(anim)
        }
    }

    // Loading or broken: a glyph on the tint.
    Icon {
        anchors.centerIn: parent
        visible: !root.hidden && root.picture !== null && root.picture.status !== Image.Ready
        name: root.picture !== null && root.picture.status === Image.Error ? "imageBroken" : "image"
        size: Theme.iconXl
        color: Theme.ink3
    }

    Rectangle {
        anchors.fill: parent
        visible: hover.containsMouse && !root.hidden
        color: Theme.tint1
    }

    // "+N" over the tenth tile.
    Rectangle {
        anchors.fill: parent
        visible: root.more > 0
        color: Theme.scrim
        Rectangle {
            anchors.centerIn: parent
            width: moreText.implicitWidth + 16
            height: 28
            radius: Theme.radius
            color: Theme.ply3
            Text {
                id: moreText
                anchors.centerIn: parent
                textFormat: Text.PlainText
                text: "+" + root.more
                color: Theme.ink1
                font: Theme.type.num
            }
        }
    }

    // Sensitive: the frame over the blur.
    Rectangle {
        anchors.fill: parent
        visible: root.hidden
        color: Theme.scrim
        Column {
            anchors.centerIn: parent
            width: Math.min(parent.width - 24, 220)
            spacing: 4
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: qsTr("Sensitive media")
                color: Theme.ink1
                font: Theme.type.panelTitle
            }
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                visible: root.height >= 120
                text: qsTr("Hidden by your high media filter.")
                color: Theme.ink2
                font: Theme.type.caption
            }
            Item { width: 1; height: 4 }
            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                width: showText.implicitWidth + 24
                height: 28
                radius: Theme.radius
                color: showHover.containsMouse ? Theme.tint3 : Theme.tint2
                Accessible.role: Accessible.Button
                Accessible.name: showText.text
                Text {
                    id: showText
                    anchors.centerIn: parent
                    text: qsTr("Show media")
                    color: Theme.ink1
                    font: Theme.type.captionStrong
                }
                MouseArea {
                    id: showHover
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: ShellState.reveal(root.item.id)
                }
            }
        }
    }

    MouseArea {
        id: hover
        anchors.fill: parent
        enabled: !root.hidden
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.activated()
    }
}
