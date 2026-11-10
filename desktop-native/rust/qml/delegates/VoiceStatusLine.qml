import QtQuick
import Yap.Media
import com.yap.native
import Yap.Ui

// A voice channel's status after its people (ChannelSidebar.tsx), indented
// to the name (pl-7), 2 over and 4 under. Once we are connected there it is
// ours to change: the status line reads the status (ink-2) or "Set a
// channel status" (ink-3) with the edit glyph after it, and the picture
// key reads "Set the vibe" until there is one. Anyone else sees the
// status and the picture only.
Column {
    id: root

    property string channelId
    property string channelName
    property string text
    property string media
    property bool editable: false

    leftPadding: 28
    topPadding: 2
    bottomPadding: 4
    spacing: 4

    Text {
        visible: !root.editable && root.text.length > 0
        width: root.width - root.leftPadding - 8
        height: 24
        leftPadding: 6
        rightPadding: 6
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
        textFormat: Text.PlainText
        text: root.text
        color: Theme.ink2
        font: Theme.type.captionStrong
    }

    StatusKey {
        visible: root.editable
        maxWidth: root.width - root.leftPadding - 8
        label: root.text.length > 0 ? root.text : qsTr("Set a channel status")
        labelInk: root.text.length > 0 ? Theme.ink2 : Theme.ink3
        trailingGlyph: "edit"
        tip: root.text.length > 0 ? qsTr("Edit status") : qsTr("Set status")
        onClicked: ShellState.voiceStatusRequested(root.channelId, root.channelName, false, root.text)
    }

    // The channel picture (VoiceChannelMediaDisplay): the full width at
    // its preview's aspect, contained on ply-2 inside a 1 px line. A
    // picture plays as an <img> does; a clip rests on its first frame and
    // plays only while pointed at. Ours to change: Change media and Remove
    // media, 28 px ply-3 keys 6 px in at the top right, shown on hover or
    // focus.
    // Parsed in one binding from `media` alone: split over several
    // (the "#" position, then the URL from it) a new picture could be read
    // with the old one's "#" position or type for a moment, and the picture
    // or the clip player was handed a cut-off URL or the other kind's file.
    // A clip that failed that way stayed blank (ClipPreview's `failed`).
    readonly property var mediaInfo: {
        const at = media.lastIndexOf("#")
        const tail = at >= 0 ? media.slice(at + 1) : ""
        const semi = tail.lastIndexOf(";")
        const size = semi >= 0 ? tail.slice(semi + 1).split("x") : []
        const type = semi >= 0 ? tail.slice(0, semi) : ""
        return {
            url: at >= 0 ? media.slice(0, at) : media,
            type: type,
            video: type.indexOf("video/") === 0,
            w: Math.max(1, Number(size[0]) || 1),
            h: Math.max(1, Number(size[1]) || 1)
        }
    }
    // Remove media is in flight until the picture goes.
    property bool removing: false
    onMediaChanged: removing = false

    Rectangle {
        id: plate
        visible: root.mediaInfo.url.length > 0 && !(still.status === Image.Error && !root.mediaInfo.video)
        // w-full of the pl-7 column: to the sidebar's edge.
        width: root.width - root.leftPadding
        height: Math.round(width * root.mediaInfo.h / root.mediaInfo.w)
        radius: Theme.radius
        color: Theme.ply2
        border.width: 1
        border.color: Theme.line
        clip: true
        Accessible.name: root.mediaInfo.video ? qsTr("Voice channel media preview") : qsTr("Voice channel status media")

        HoverHandler { id: plateHover }
        // YAP_TEST_HOVER_PLAY: as if pointed at (tests only).
        readonly property bool pointed: plateHover.hovered || ShellState.testHoverPlay

        // A status GIF loops (AnimatedPicture `stream`: Qt 6.11's
        // AnimatedImage plays a network file from its reply, which cannot
        // rewind, so it stopped after one pass). Its Error is Image.Error's.
        AnimatedPicture {
            id: still
            anchors.fill: parent
            anchors.margins: 1
            visible: !root.mediaInfo.video
            fillMode: AnimatedPicture.PreserveAspectFit
            stream: true
            source: root.mediaInfo.video ? "" : root.mediaInfo.url
            playing: visible && !Theme.reducedMotion
        }
        ClipPreview {
            anchors.fill: parent
            anchors.margins: 1
            visible: root.mediaInfo.video
            source: root.mediaInfo.video ? root.mediaInfo.url : ""
            contentType: root.mediaInfo.type
            active: (plate.pointed || changeKey.activeFocus || removeKey.activeFocus) && !Theme.reducedMotion
        }

        Row {
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 6
            z: 2
            spacing: 4
            visible: root.editable
            opacity: plate.pointed || changeKey.activeFocus || removeKey.activeFocus ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: Theme.dur1 } }
            PlateKey {
                id: changeKey
                glyph: "edit"
                tip: qsTr("Change media")
                onClicked: ShellState.voiceStatusRequested(root.channelId, root.channelName, true, root.text)
            }
            PlateKey {
                id: removeKey
                glyph: "trash"
                tip: qsTr("Remove media")
                danger: true
                busy: root.removing
                onClicked: {
                    root.removing = true
                    Extras.removeVoiceMedia(root.channelId)
                }
            }
        }
    }

    // size-7, rounded-md, a 1 px line on ply-3 with shadow-float, ink-2
    // (hover ink-1, or danger ink for Remove).
    component PlateKey: Rectangle {
        id: plateKey
        property string glyph
        property string tip
        property bool danger: false
        property bool busy: false
        signal clicked()
        width: 28
        height: 28
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.line
        scale: plateTap.pressed ? Theme.scalePress : 1
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: tip
        Keys.onReturnPressed: if (!busy) plateKey.clicked()
        Keys.onSpacePressed: if (!busy) plateKey.clicked()
        FloatShadow {}
        Icon {
            anchors.centerIn: parent
            visible: !plateKey.busy
            name: plateKey.glyph
            size: Theme.iconSm
            color: plateKeyHover.hovered ? (plateKey.danger ? Theme.dangerInk : Theme.ink1) : Theme.ink2
        }
        Icon {
            id: plateSpin
            anchors.centerIn: parent
            visible: plateKey.busy
            name: "loader"
            size: Theme.iconSm
            color: Theme.ink2
            RotationAnimator on rotation {
                from: 0
                to: 360
                duration: Theme.durSpin
                loops: Animation.Infinite
                running: plateSpin.visible && !Theme.reducedMotion
            }
        }
        Rectangle {
            anchors.fill: parent
            anchors.margins: -2
            visible: plateKey.activeFocus
            color: "transparent"
            radius: Theme.radius
            border.width: 2
            border.color: Theme.focus
        }
        LazyPopup {
            when: plateKeyHover.hovered
            sourceComponent: Tooltip {
                visible: plateKeyHover.hovered
                text: plateKey.tip
            }
        }
        HoverHandler { id: plateKeyHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { id: plateTap; enabled: !plateKey.busy; onTapped: plateKey.clicked() }
    }

    StatusKey {
        visible: root.editable && root.media.length === 0
        maxWidth: root.width - root.leftPadding - 8
        leadingGlyph: "sparkle"
        label: qsTr("Set the vibe")
        labelInk: Theme.ink3
        onClicked: ShellState.voiceStatusRequested(root.channelId, root.channelName, true, root.text)
    }

    // h-6 px-1.5 gap-1.5, 12/500; hover tint-1, the glyph and the quiet
    // label go ink-1.
    component StatusKey: Rectangle {
        id: key
        property string label
        property color labelInk: Theme.ink3
        property string leadingGlyph: ""
        property string trailingGlyph: ""
        property string tip: ""
        property real maxWidth: 200
        signal clicked()
        width: Math.min(maxWidth, keyRow.implicitWidth + 12)
        height: 24
        radius: Theme.radius
        color: keyHover.hovered ? Theme.tint1 : "transparent"
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: tip.length > 0 ? tip + ": " + label : label
        Keys.onReturnPressed: key.clicked()
        Keys.onSpacePressed: key.clicked()
        Row {
            id: keyRow
            x: 6
            width: Math.min(implicitWidth, key.maxWidth - 12)
            anchors.verticalCenter: parent.verticalCenter
            spacing: 6
            Icon {
                visible: key.leadingGlyph.length > 0
                anchors.verticalCenter: parent.verticalCenter
                name: key.leadingGlyph || "sparkle"
                size: Theme.iconXs
                color: keyHover.hovered ? Theme.ink1 : Theme.ink3
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, key.maxWidth - 12 - (key.leadingGlyph.length > 0 || key.trailingGlyph.length > 0 ? 18 : 0))
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: key.label
                color: key.leadingGlyph.length > 0 && keyHover.hovered ? Theme.ink1 : key.labelInk
                EaseColor on color {}
                font: Theme.type.captionStrong
            }
            Icon {
                visible: key.trailingGlyph.length > 0
                anchors.verticalCenter: parent.verticalCenter
                name: key.trailingGlyph || "edit"
                size: Theme.iconXs
                color: keyHover.hovered ? Theme.ink1 : Theme.ink3
            }
        }
        Rectangle {
            anchors.fill: parent
            visible: key.activeFocus
            color: "transparent"
            radius: Theme.radius
            border.width: 2
            border.color: Theme.focus
        }
        LazyPopup {
            // Made while hovered.
            when: keyHover.hovered && key.tip.length > 0
            sourceComponent: Tooltip {
                visible: keyHover.hovered && key.tip.length > 0
                text: key.tip
            }
        }
        HoverHandler { id: keyHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: key.clicked() }
    }
}
