import QtQuick
import com.yap.native

// What the call is listening to, under our voice channel's people
// (ListenTogetherSidebarStatus.tsx): ml-7 mr-2 mb-1, px-1 py-1, gap-2; a
// 28 px tint-1 artwork (else the music glyph in ink-3), then the play or
// pause glyph (12, Fill, ink-3) and the title at 12/500 ink-2 over the
// clock (Num ink-3, the elapsed time; 0:00 for a track with no length).
// Nothing while nothing is queued. The clock ticks once a second only
// while a track with a length plays.
Item {
    id: root

    readonly property var parsed: JSON.parse(Apps.state || "{}")
    readonly property var l: parsed.listen ?? ({})
    readonly property var now: l.now ?? null
    property real position: 0

    function tick() {
        const base = l.positionMs ?? 0
        const since = l.paused ? 0 : Math.max(0, Apps.nowMs() - (l.positionAt ?? 0))
        // clampListenTogetherPosition: a track with no length (a live
        // radio) reads 0:00, playing or paused, as on the web.
        const duration = Math.max(0, l.durationMs ?? 0)
        position = Math.min(duration, Math.max(0, base + since))
    }
    function clock(ms) {
        const total = Math.max(0, Math.floor(ms / 1000))
        return Math.floor(total / 60) + ":" + String(total % 60).padStart(2, "0")
    }
    onLChanged: tick()
    Component.onCompleted: tick()

    // Its own flag: `visible` also folds in the parent's.
    readonly property bool shown: now !== null
    visible: shown
    width: parent ? parent.width : 0
    height: shown ? 36 + 4 : 0

    Timer {
        interval: 1000
        repeat: true
        // isListenTogetherProgressRunning: only a track with a length runs.
        running: root.shown && root.l.paused === false && (root.l.durationMs ?? 0) > 0
        onTriggered: root.tick()
    }

    Row {
        x: 28 + 4
        y: 4
        width: root.width - 28 - 8 - 8
        spacing: 8
        Rectangle {
            width: 28
            height: 28
            radius: Theme.radius
            color: Theme.tint1
            clip: true
            Icon {
                anchors.centerIn: parent
                visible: art.status !== Image.Ready
                name: "music"
                size: Theme.iconSm
                color: Theme.ink3
            }
            Image {
                id: art
                anchors.fill: parent
                source: root.now?.artwork ?? ""
                sourceSize: Qt.size(56, 56)
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
            }
        }
        Column {
            width: parent.width - 28 - 8
            anchors.verticalCenter: parent.verticalCenter
            Row {
                width: parent.width
                spacing: 6
                Icon {
                    anchors.verticalCenter: parent.verticalCenter
                    name: root.l.paused ? "pause" : "play"
                    variant: "active"
                    size: Theme.iconXs
                    color: Theme.ink3
                }
                Text {
                    width: parent.width - Theme.iconXs - 6
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                    text: root.now?.title ?? ""
                    color: Theme.ink2
                    font: Theme.type.captionStrong
                }
            }
            Text {
                textFormat: Text.PlainText
                text: root.clock(root.position)
                color: Theme.ink3
                font: Theme.type.numSm
            }
        }
    }
}
