import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// A voice channel's status after its people, mobile side (the desktop's
// VoiceStatusLine at touch size), indented to the name. Once we are
// connected there it is ours to change: the status (ink-2) or "Set a
// channel status" (ink-3) with the edit glyph, and the vibe, or "Set the
// vibe" until there is one; a tap opens the status sheet on that half
// (VoiceStatusSheet, through ShellState.voiceStatusRequested). Anyone else
// sees the status and the vibe only. The vibe is the picture at its own
// shape, at most 160 high; a clip rests on its first frame.
Column {
    id: root

    property string channelId: ""
    property string channelName: ""
    property string text: ""
    property string media: ""
    property bool editable: false

    VoiceStatusMedia {
        id: parsed
        media: root.media
    }
    readonly property var info: parsed.info
    readonly property real lineWidth: width - leftPadding - 8

    function edit(media) {
        Native.haptic(Native.Tick)
        ShellState.voiceStatusRequested(root.channelId, root.channelName, media, root.text)
    }

    leftPadding: 28
    topPadding: 2
    bottomPadding: 6
    spacing: 4

    Text {
        visible: !root.editable && root.text.length > 0
        width: root.lineWidth
        leftPadding: 6
        rightPadding: 6
        elide: Text.ElideRight
        textFormat: Text.PlainText
        text: root.text
        color: Theme.ink2
        font: Theme.type.captionStrong
    }

    StatusKey {
        visible: root.editable
        label: root.text.length > 0 ? root.text : qsTr("Set a channel status")
        labelInk: root.text.length > 0 ? Theme.ink2 : Theme.ink3
        glyph: "edit"
        trailing: true
        spoken: root.text.length > 0 ? qsTr("Edit status: %1").arg(root.text) : qsTr("Set a channel status")
        onTapped: root.edit(false)
    }

    Rectangle {
        id: plate
        visible: root.info.url.length > 0 && !vibe.failed
        width: Math.min(root.width - root.leftPadding - 8, 160 * root.info.w / root.info.h)
        height: Math.round(width * root.info.h / root.info.w)
        radius: Theme.radius
        color: Theme.ply2
        border.width: 1
        border.color: plateTap.pressed ? Theme.ink2 : Theme.line
        clip: true
        Accessible.role: root.editable ? Accessible.Button : Accessible.Graphic
        Accessible.name: root.editable ? qsTr("Change the vibe") : qsTr("Voice channel status media")
        Accessible.onPressAction: if (root.editable) root.edit(true)
        VibePicture {
            id: vibe
            anchors.fill: parent
            anchors.margins: 1
            url: root.info.url
            video: root.info.video
            playing: root.visible && Qt.application.state === Qt.ApplicationActive
        }
        TapHandler {
            id: plateTap
            enabled: root.editable
            onTapped: root.edit(true)
        }
    }

    StatusKey {
        visible: root.editable && root.media.length === 0
        label: qsTr("Set the vibe")
        labelInk: Theme.ink3
        glyph: "sparkle"
        spoken: label
        onTapped: root.edit(true)
    }

    // A tappable line: 32 high (a 44 touch), the glyph before or after.
    component StatusKey: Rectangle {
        id: key
        property string label
        property color labelInk: Theme.ink3
        property string glyph
        property bool trailing: false
        property string spoken
        signal tapped()
        width: Math.min(root.lineWidth, keyRow.implicitWidth + 12)
        height: 32
        radius: Theme.radius
        color: keyTap.pressed ? Theme.tint2 : "transparent"
        Accessible.role: Accessible.Button
        Accessible.name: spoken
        Accessible.onPressAction: key.tapped()
        Row {
            id: keyRow
            x: 6
            anchors.verticalCenter: parent.verticalCenter
            layoutDirection: key.trailing ? Qt.RightToLeft : Qt.LeftToRight
            spacing: 6
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: key.glyph
                size: Theme.iconXs
                color: Theme.ink3
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, root.lineWidth - 12 - Theme.iconXs - 6)
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: key.label
                color: key.labelInk
                font: Theme.type.captionStrong
            }
        }
        TapHandler {
            id: keyTap
            margin: 6
            onTapped: key.tapped()
        }
    }
}
