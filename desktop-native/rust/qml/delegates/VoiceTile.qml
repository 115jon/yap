import QtQuick
import QtQuick.Templates as T
import QtMultimedia
import com.yap.native

// One tile on the call stage (ParticipantCard.tsx, TileParts.tsx): a person,
// or someone's share. A ply-2 surface with a 1 px seam (accent while on
// stage) and 2 px corners. A camera that is on shows its video cropped to
// the tile, a watched share letterboxed on black; until the first frame a
// spinner stands in. An unwatched share shows "<name> is sharing" and
// Watch. Our own share shows its capture as sent, with Pause / Resume for
// the preview at the top left (on hover). Anyone without media gets the
// 64 px avatar (80 alone). The
// nameplate sits bottom-left: LED, Bricolage 13/700 name, state glyphs, on a
// black/60 chip over media. A person's speaking ring is a 2 px talk outline
// outside the tile. Click puts the tile on stage (or takes it off); right
// click opens their call menu.
Item {
    id: root

    required property string tileId
    required property string kind
    // The seat key (user id) for the call menu.
    required property string memberId
    required property string participantId
    required property string name
    required property string avatar
    // The person's avatar decoration (APNG), or empty.
    required property string deco
    required property bool muted
    required property bool deafened
    required property bool speaking
    required property bool me
    required property bool video
    required property string videoKey
    required property bool watching
    // Shares: who watches (JSON), shown top right while it is ours or
    // watched (ParticipantCard.tsx `shouldShowWatchers`).
    required property string watchers

    property bool solo: false
    // A members-tray thumbnail (144 px): small avatar, no Watch prompt.
    property bool tray: false
    readonly property bool focused: ShellState.focusedTile === tileId
    readonly property bool screen: kind === "screen"
    // Our own share, and its preview paused.
    readonly property bool ownShare: screen && me
    readonly property bool previewPaused: ownShare && ShellState.sharePreviewPaused
    // The tile on stage keeps only its avatar in the tray.
    readonly property bool showsVideo: video && videoKey.length > 0 && !(tray && focused)
        && (!screen || me || watching || focused) && !previewPaused
    readonly property bool watchPrompt: screen && !me && !watching && !focused && !tray
    readonly property bool overMedia: showsVideo && surface.item !== null && surface.item.hasFrame
    readonly property string ownerName: name

    Accessible.role: Accessible.Button
    Accessible.name: ownShare ? qsTr("Your stream") : screen ? qsTr("%1's stream").arg(name) : name
    Accessible.onPressAction: ShellState.toggleFocus(tileId)

    TapHandler {
        onTapped: ShellState.toggleFocus(root.tileId)
    }
    TapHandler {
        enabled: !root.me
        acceptedButtons: Qt.RightButton
        onTapped: eventPoint => ShellState.showPeerMenu(root.memberId, root.name, root, eventPoint.position, Voice.channelId)
    }
    HoverHandler { id: hover }

    Rectangle {
        id: surfaceBox
        anchors.fill: parent
        radius: Theme.radius
        color: root.overMedia ? "black" : Theme.ply2
        border.width: 1
        border.color: root.focused ? Theme.accentInk : hover.hovered ? Theme.lineStrong : Theme.line

        Loader {
            id: surface
            anchors.fill: parent
            anchors.margins: 1
            active: root.showsVideo
            sourceComponent: VideoSurface {
                videoKey: root.videoKey
                fillMode: root.screen ? VideoOutput.PreserveAspectFit : VideoOutput.PreserveAspectCrop
            }
        }

        StreamLoading {
            anchors.fill: parent
            visible: root.showsVideo && !root.overMedia
            name: root.ownerName
        }
    }

    // An unwatched share: Watch.
    Column {
        visible: root.watchPrompt
        anchors.centerIn: parent
        width: parent.width - 16
        spacing: 8
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            width: 32
            height: 32
            radius: Theme.radius
            color: Theme.liveTint
            Icon {
                anchors.centerIn: parent
                name: "screenShare"
                variant: "active"
                size: Theme.iconSm
                color: Theme.liveInk
            }
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            elide: Text.ElideRight
            textFormat: Text.PlainText
            text: qsTr("%1 is sharing").arg(root.ownerName)
            color: Theme.ink1
            font: Theme.type.captionStrong
        }
        SecondaryKey {
            anchors.horizontalCenter: parent.horizontalCenter
            width: Math.min(implicitWidth, parent.width)
            text: qsTr("Watch stream")
            onClicked: Voice.watch(root.participantId, true)
        }
    }

    // Our share's preview, paused to save resources.
    Column {
        visible: root.previewPaused
        anchors.centerIn: parent
        spacing: 8
        Icon {
            anchors.horizontalCenter: parent.horizontalCenter
            name: "eyeOff"
            size: Theme.iconLg
            color: Theme.ink3
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: qsTr("Preview paused")
            color: Theme.ink2
            font: Theme.type.captionStrong
        }
    }

    // Our share: pause or resume its preview (shown on hover or focus).
    T.AbstractButton {
        id: previewKey
        visible: root.ownShare && !root.tray && (hover.hovered || visualFocus)
        x: 8
        y: 8
        implicitWidth: previewRow.implicitWidth + 16
        implicitHeight: 24
        focusPolicy: Qt.StrongFocus
        Accessible.role: Accessible.Button
        Accessible.name: root.previewPaused ? qsTr("Resume preview") : qsTr("Pause preview")
        onClicked: ShellState.sharePreviewPaused = !ShellState.sharePreviewPaused
        contentItem: Row {
            id: previewRow
            x: 8
            spacing: 6
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: root.previewPaused ? "eye" : "eyeOff"
                size: Theme.iconXs
                color: "white"
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: root.previewPaused ? qsTr("Resume") : qsTr("Pause")
                color: "white"
                font: Theme.type.captionStrong
            }
        }
        background: Rectangle {
            radius: Theme.radius
            color: Qt.rgba(0, 0, 0, previewKey.down ? 0.75 : previewKey.hovered ? 0.7 : 0.6)
            border.width: previewKey.visualFocus ? 2 : 0
            border.color: Theme.focus
        }
    }

    Avatar {
        visible: !root.showsVideo && !root.watchPrompt && !root.previewPaused
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.verticalCenter: root.tray ? undefined : parent.verticalCenter
        y: root.tray ? 8 : 0
        size: root.tray ? 40 : root.solo ? 80 : 64
        initials: root.name
        source: root.avatar
        deco: root.deco
    }

    // Speaking: a 2 px talk outline outside the tile (people only).
    Rectangle {
        visible: !root.screen
        anchors.fill: parent
        anchors.margins: -2
        color: "transparent"
        radius: Theme.radius
        border.width: 2
        border.color: Theme.talk
        opacity: root.speaking ? 1 : 0
        Behavior on opacity { OpacityAnimator { duration: root.speaking ? Theme.speakOn : Theme.speakOff } }
    }

    // Nameplate: h 24, px 6, on a black/60 chip over media.
    Rectangle {
        x: root.tray ? 4 : 8
        anchors.bottom: parent.bottom
        anchors.bottomMargin: root.tray ? 4 : 8
        width: plate.implicitWidth + 12
        height: 24
        radius: Theme.radius
        color: root.overMedia ? Qt.rgba(0, 0, 0, 0.6) : "transparent"
        Row {
            id: plate
            x: 6
            anchors.verticalCenter: parent.verticalCenter
            spacing: 6
            Led {
                anchors.verticalCenter: parent.verticalCenter
                visible: !root.screen
                lit: root.speaking && !root.muted && !root.deafened
                color: Theme.talk
            }
            Text {
                id: label
                anchors.verticalCenter: parent.verticalCenter
                // Room for the chip's padding, the LED and the glyphs.
                width: Math.max(0, Math.min(implicitWidth, root.width - (root.tray ? 8 : 48) - 12 - 12
                    - (root.screen ? 18 : 0) - (root.muted ? 18 : 0) - (root.deafened ? 18 : 0)))
                // A share is named for its owner (TileNameplate's ownerName).
                textFormat: Text.PlainText
                text: root.me && !root.screen ? qsTr("%1 (you)").arg(root.name) : root.name
                elide: Text.ElideRight
                color: root.overMedia ? "white" : Theme.ink1
                font.family: "Yap Bricolage Key Bold"
                font.pixelSize: 13
                font.weight: Font.Bold
            }
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                visible: root.screen
                name: "screenShare"
                size: Theme.iconXs
                color: root.overMedia ? Qt.rgba(1, 1, 1, 0.8) : Theme.ink3
            }
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                visible: root.muted && !root.screen
                name: "mic"
                variant: "off"
                size: Theme.iconXs
                color: Theme.mutedState
            }
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                visible: root.deafened && !root.screen
                name: "deafen"
                variant: "off"
                size: Theme.iconXs
                color: Theme.mutedState
            }
        }
    }

    StreamWatcherList {
        anchors.top: parent.top
        anchors.right: parent.right
        anchors.margins: 8
        z: 4
        width: Math.min(implicitWidth, parent.width * 0.75)
        visible: root.screen && !root.tray && (root.me || root.watching) && root.watchers.length > 0
        watchers: root.watchers
    }
}
