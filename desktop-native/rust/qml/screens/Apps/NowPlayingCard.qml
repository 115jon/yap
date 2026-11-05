import QtQuick
import com.yap.native

// What the room is listening to (ListenTogetherNowPlayingCard.tsx): the
// artwork, title over artist, a progress bar that seeks for everyone, the
// clock, play or pause, and skip. The clock runs locally from the room's
// position between updates.
Rectangle {
    id: root

    // The state is parsed afresh on every change, and a new object would
    // rebuild every Repeater under it; this slice is replaced only when its
    // content differs (as FriendsView and SearchPanel do).
    readonly property var parsed: JSON.parse(Apps.state || "{}")
    property var l: ({})
    property string lKey: "{}"

    function syncL() {
        const next = parsed.listen ?? {}
        const key = JSON.stringify(next)
        if (key !== lKey) {
            lKey = key
            l = next
        }
    }
    onParsedChanged: syncL()
    Component.onCompleted: syncL()
    readonly property var now: l.now ?? null
    property real position: 0

    function tick() {
        const base = l.positionMs ?? 0
        const since = l.paused ? 0 : Math.max(0, Apps.nowMs() - (l.positionAt ?? 0))
        const duration = l.durationMs ?? 0
        position = duration > 0 ? Math.min(duration, base + since) : base + since
    }
    function clock(ms) {
        const total = Math.max(0, Math.floor(ms / 1000))
        return qsTr("%1:%2").arg(Math.floor(total / 60)).arg(String(total % 60).padStart(2, "0"))
    }

    visible: now !== null
    implicitHeight: visible ? 64 : 0
    color: Theme.ply2
    onLChanged: tick()

    Timer {
        interval: 1000
        repeat: true
        running: root.visible && root.l.paused === false
        onTriggered: root.tick()
    }
    Rectangle { width: parent.width; height: 1; color: Theme.line }

    TrackRow {
        anchors.left: parent.left
        anchors.right: keys.left
        anchors.leftMargin: 8
        anchors.top: parent.top
        anchors.topMargin: 2
        title: root.now?.title ?? ""
        artist: root.now?.artist ?? ""
        artwork: root.now?.artwork ?? ""
    }
    Row {
        id: keys
        anchors.right: parent.right
        anchors.rightMargin: 16
        anchors.top: parent.top
        anchors.topMargin: 10
        spacing: 4
        Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: qsTr("%1 / %2").arg(root.clock(root.position)).arg(root.clock(root.l.durationMs ?? 0))
            color: Theme.ink3
            font: Theme.type.numSm
        }
        IconKey {
            size: 32
            iconName: root.l.paused ? "play" : "pause"
            tone: "rest"
            tip: root.l.paused ? qsTr("Play") : qsTr("Pause")
            onClicked: Apps.ltPause(!root.l.paused)
        }
        IconKey {
            size: 32
            iconName: "skip"
            tone: "rest"
            tip: qsTr("Skip")
            onClicked: Apps.ltSkip()
        }
    }
    // The progress bar: a click or drag seeks the room.
    Item {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.leftMargin: 16
        anchors.rightMargin: 16
        height: 10
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width
            height: 3
            radius: 2
            color: Theme.tint3
            Rectangle {
                width: (root.l.durationMs ?? 0) > 0 ? parent.width * Math.min(1, root.position / root.l.durationMs) : 0
                height: parent.height
                radius: 2
                color: Theme.primaryFill
            }
        }
        MouseArea {
            anchors.fill: parent
            anchors.topMargin: -4
            cursorShape: Qt.PointingHandCursor
            Accessible.role: Accessible.Slider
            Accessible.name: qsTr("Seek")
            onReleased: mouse => {
                const duration = root.l.durationMs ?? 0
                if (duration > 0)
                    Apps.ltSeek(Math.max(0, Math.min(1, mouse.x / width)) * duration)
            }
        }
    }
}
