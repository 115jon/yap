pragma ComponentBehavior: Bound
import QtQuick
import Yap.Ui

// What is wrong with the call, one line each, under its header: the audio
// sentinels (RoomVoiceView.tsx: nothing goes out of our unmuted mic, or
// nothing comes in while someone talks), a dropped call link while it
// comes back, and a poor link. Nothing at all while the call is fine.
// Lines on the danger tint for audio that stopped, the warning tint for a
// link that is weak or coming back; a glyph, then the line in its ink.
//
// The desktop's lines are one 32 px row each (`wrap: false`); a phone's
// wrap to the width and grow (`wrap: true`, with the window's insets).
// `alert` is a line of the caller's own put first (a failed call's reason,
// a camera that would not start), `alertDanger` its tint. While the call
// reconnects only that line shows: audio stalls are what a drop looks like.
Column {
    id: root

    property bool micStalled: false
    property bool inboundStalled: false
    property bool reconnecting: false
    property bool poorLink: false
    property string alert: ""
    property bool alertDanger: true

    property string micText: qsTr("We can't hear you. Your microphone isn't picking up any sound.")
    property string inboundText: qsTr("You can't hear anyone. Check your output device.")
    property string reconnectingText: qsTr("The call's connection dropped. Yap is reconnecting you.")
    property string poorLinkText: qsTr("Your connection is poor. Others may hear you cut out.")

    property bool wrap: false
    property font textFont: Theme.type.caption
    property real leftInset: 0
    property real rightInset: 0

    readonly property var lines: {
        const out = []
        if (alert.length > 0)
            out.push({ "icon": alertDanger ? "error" : "warning", "text": alert, "danger": alertDanger })
        if (reconnecting) {
            out.push({ "icon": "refresh", "text": reconnectingText, "danger": false })
            return out
        }
        if (micStalled)
            out.push({ "icon": "micOff", "text": micText, "danger": true })
        if (inboundStalled)
            out.push({ "icon": "deafen", "text": inboundText, "danger": true })
        if (poorLink)
            out.push({ "icon": "signal", "text": poorLinkText, "danger": false })
        return out
    }
    readonly property bool shown: lines.length > 0

    Repeater {
        model: root.lines
        Rectangle {
            id: line
            required property var modelData
            readonly property color ink: modelData.danger ? Theme.dangerInk : Theme.warningInk
            width: root.width
            height: root.wrap ? Math.max(32, words.implicitHeight + 16) : 32
            color: modelData.danger ? Theme.dangerTint : Theme.warningTint
            Row {
                x: 16 + root.leftInset
                anchors.verticalCenter: parent.verticalCenter
                spacing: 8
                Icon {
                    anchors.verticalCenter: root.wrap ? undefined : parent.verticalCenter
                    // On the first line, even when the words wrap.
                    y: root.wrap ? Math.max(0, (words.lineHeight0 - height) / 2) : 0
                    name: line.modelData.icon
                    size: Theme.iconSm
                    color: line.ink
                }
                Text {
                    id: words
                    // The first line's height, for the glyph beside it.
                    readonly property real lineHeight0: lineCount > 0 ? implicitHeight / lineCount : implicitHeight
                    anchors.verticalCenter: root.wrap ? undefined : parent.verticalCenter
                    width: root.wrap ? line.width - 32 - root.leftInset - root.rightInset - Theme.iconSm - 8 : implicitWidth
                    wrapMode: root.wrap ? Text.Wrap : Text.NoWrap
                    textFormat: Text.PlainText
                    text: line.modelData.text
                    color: line.ink
                    font: root.textFont
                    Accessible.role: Accessible.AlertMessage
                }
            }
        }
    }
}
