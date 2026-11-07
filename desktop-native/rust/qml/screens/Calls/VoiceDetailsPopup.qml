pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// Voice details (VoiceDetailsPanel.tsx), anchored above its trigger: the
// Connection tab (the transport's state three up, the quality word, the
// small multiples of bitrate, loss, jitter and round trip over the last
// 24 ticks, the SFU edge, session loss and remote tracks, and the web's
// note) and the Privacy tab; our outgoing video's encoder, size and rate
// while the camera or a share is on (StreamingStatsPanel.tsx); the footer
// says Encrypted while connected, with Copy stats. Numbers come from the
// call's 5 s stats.
Popup {
    id: root

    readonly property var calls: JSON.parse(Calls.view)
    readonly property var stats: calls.stats ?? null
    property string tab: "connection"
    readonly property string state: Voice.live ? "connected" : Voice.channelId.length > 0 ? "connecting" : "disconnected"
    readonly property var quality: ({
        good: { led: "talk", text: qsTr("Stable") },
        warn: { led: "idle", text: qsTr("Unsteady") },
        bad: { led: "muted", text: qsTr("Poor") }
    })[stats ? stats.quality : ""] ?? { led: "off", text: qsTr("Connecting") }

    function toggleAt(anchor) {
        if (root.opened) {
            root.close()
            return
        }
        const win = Overlay.overlay
        const at = anchor.mapToItem(win, 0, 0)
        x = Math.max(8, Math.min(win.width - width - 8, at.x))
        y = Math.max(8, at.y - height - 8)
        open()
    }

    function stateLabel(s) {
        return s === "connected" ? qsTr("Connected") : s === "connecting" ? qsTr("Connecting") : qsTr("Disconnected")
    }

    parent: Overlay.overlay
    width: 360
    height: column.implicitHeight
    padding: 0
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onOpened: Calls.setDetailsOpen(true)
    onClosed: Calls.setDetailsOpen(false)

    background: Rectangle {
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        FloatShadow {}
        Rectangle { width: parent.width; height: 2; color: Theme.ink1 }
    }

    ClipboardText { id: clipboard }

    contentItem: Column {
        id: column
        width: root.width

        Text {
            x: 16
            topPadding: 16
            bottomPadding: 8
            textFormat: Text.PlainText
            text: qsTr("Voice details")
            color: Theme.ink1
            font: Theme.type.sectionTitle
            Accessible.role: Accessible.Heading
        }

        // Tabs.
        Item {
            width: parent.width
            height: 40
            Row {
                x: 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 4
                Accessible.role: Accessible.PageTabList
                Accessible.name: qsTr("Voice details sections")
                Repeater {
                    model: [{ id: "connection", text: qsTr("Connection") }, { id: "privacy", text: qsTr("Privacy") }]
                    Rectangle {
                        id: tabKey
                        required property var modelData
                        readonly property bool active: root.tab === modelData.id
                        width: tabText.implicitWidth + 20
                        height: 28
                        radius: Theme.radius
                        color: active ? Theme.tint2 : tabHover.hovered ? Theme.tint1 : "transparent"
                        Accessible.role: Accessible.PageTab
                        Accessible.name: modelData.text
                        Text {
                            id: tabText
                            anchors.centerIn: parent
                            textFormat: Text.PlainText
                            text: tabKey.modelData.text
                            color: tabKey.active ? Theme.ink1 : Theme.ink3
                            font: Theme.type.control
                        }
                        HoverHandler { id: tabHover; cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: root.tab = tabKey.modelData.id }
                    }
                }
            }
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line }
        }

        // Connection.
        Column {
            visible: root.tab === "connection"
            x: 16
            width: parent.width - 32
            topPadding: 12
            bottomPadding: 12
            spacing: 12

            // Session, Publish, Receive: one peer connection here.
            Row {
                width: parent.width
                Repeater {
                    model: [qsTr("Session"), qsTr("Publish"), qsTr("Receive")]
                    Column {
                        required property string modelData
                        width: parent.width / 3
                        spacing: 6
                        Text {
                            textFormat: Text.PlainText
                            text: parent.modelData
                            color: Theme.ink3
                            font: Theme.type.silk
                        }
                        LedLabel {
                            text: root.stateLabel(root.state)
                            ledState: root.state === "connected" ? "talk" : root.state === "connecting" ? "idle" : "off"
                        }
                    }
                }
            }

            Text {
                visible: root.stats === null
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: root.state === "connected" ? qsTr("Connected to voice. Gathering latency and packet metrics…")
                    : qsTr("Connecting to the voice transport…")
                color: Theme.ink3
                font: Theme.type.caption
            }

            // Connection quality and the small multiples.
            Column {
                visible: root.stats !== null
                width: parent.width
                spacing: 8
                Row {
                    spacing: 8
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: qsTr("Connection")
                        color: Theme.ink1
                        font: Theme.type.captionStrong
                    }
                    LedLabel {
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.quality.text
                        ledState: root.quality.led === "muted" ? "live" : root.quality.led
                    }
                }
                Grid {
                    columns: 2
                    columnSpacing: 8
                    rowSpacing: 8
                    Repeater {
                        model: [
                            { label: qsTr("Bitrate"), unit: "kbps", max: 80, i: 0, digits: 0 },
                            { label: qsTr("Packet loss"), unit: "%", max: 4, i: 1, digits: 1 },
                            { label: qsTr("Jitter"), unit: "ms", max: 40, i: 2, digits: 0 },
                            { label: qsTr("Round trip"), unit: "ms", max: 200, i: 3, digits: 0 }
                        ]
                        Rectangle {
                            id: metric
                            required property var modelData
                            readonly property var samples: root.stats ? root.stats.samples : []
                            readonly property real last: samples.length > 0 ? samples[samples.length - 1][modelData.i] : 0
                            width: (column.width - 32 - 8) / 2
                            height: 64
                            radius: Theme.radius
                            color: Theme.tint1
                            Text {
                                x: 8
                                y: 6
                                textFormat: Text.PlainText
                                text: metric.modelData.label
                                color: Theme.ink3
                                font: Theme.type.silk
                            }
                            Text {
                                anchors.right: parent.right
                                anchors.rightMargin: 8
                                y: 4
                                textFormat: Text.PlainText
                                text: metric.last.toFixed(metric.modelData.digits) + " " + metric.modelData.unit
                                color: Theme.ink1
                                font: Theme.type.captionStrong
                            }
                            Row {
                                x: 8
                                anchors.bottom: parent.bottom
                                anchors.bottomMargin: 6
                                height: 32
                                spacing: 1
                                Repeater {
                                    model: metric.samples
                                    Rectangle {
                                        required property var modelData
                                        anchors.bottom: parent.bottom
                                        width: Math.max(2, (metric.width - 16) / 24 - 1)
                                        height: Math.max(1, Math.min(1, modelData[metric.modelData.i] / metric.modelData.max) * 32)
                                        color: Theme.accentInk
                                    }
                                }
                            }
                        }
                    }
                }
                Row {
                    width: parent.width
                    spacing: 8
                    Text {
                        textFormat: Text.PlainText
                        text: qsTr("Server")
                        color: Theme.ink3
                        font: Theme.type.silk
                    }
                    Text {
                        width: parent.width - 60
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                        text: root.stats ? (root.stats.server || "—") : ""
                        color: Theme.ink2
                        font: Theme.type.caption
                    }
                }
                Repeater {
                    model: root.stats ? [
                        { label: qsTr("Session packet loss"), value: (root.stats.packetLossRate * 100).toFixed(1) + "%" },
                        { label: qsTr("Remote tracks"), value: String(root.stats.remoteTracks) }
                    ] : []
                    Item {
                        required property var modelData
                        width: parent.width
                        height: 28
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: parent.modelData.label
                            color: Theme.ink3
                            font: Theme.type.silk
                        }
                        Text {
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: parent.modelData.value
                            color: Theme.ink1
                            font: Theme.type.caption
                        }
                        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line }
                    }
                }
                // StreamingStatsPanel: what our camera or share sends.
                Repeater {
                    model: root.stats ? root.stats.video.filter(v => v.active && v.width > 0) : []
                    Text {
                        required property var modelData
                        width: parent.width
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                        text: qsTr("Video %1%2: %3 %4×%5 at %6 fps, %7 kbps, %8%9, %10 ms, %11 dropped%12")
                            .arg(modelData.mid).arg(modelData.rid.length > 0 ? "/" + modelData.rid : "")
                            .arg(modelData.codec)
                            .arg(modelData.width).arg(modelData.height).arg(Math.round(modelData.fps))
                            .arg(Math.round(modelData.bitrateKbps))
                            .arg(modelData.encoder || qsTr("encoder pending"))
                            .arg(modelData.hardware ? qsTr(" (hardware)") : "")
                            .arg(modelData.encodeMs.toFixed(1))
                            .arg(modelData.dropped)
                            .arg(modelData.limitation !== "none" ? qsTr(", limited by %1").arg(modelData.limitation) : "")
                        color: Theme.ink2
                        font: Theme.type.caption
                    }
                }
                // What we receive, per stream.
                Repeater {
                    model: root.stats && root.stats.incoming ? root.stats.incoming.filter(v => v.width > 0) : []
                    Text {
                        required property var modelData
                        width: parent.width
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                        text: qsTr("Incoming %1: %2 %3×%4 at %5 fps, %6, %7 ms, %8 dropped")
                            .arg(modelData.key || qsTr("unrouted"))
                            .arg(modelData.codec)
                            .arg(modelData.width).arg(modelData.height).arg(Math.round(modelData.fps))
                            .arg(modelData.decoder || qsTr("decoder pending"))
                            .arg(modelData.decodeMs.toFixed(1))
                            .arg(modelData.dropped)
                        color: Theme.ink2
                        font: Theme.type.caption
                    }
                }
                Text {
                    width: parent.width
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    text: qsTr("You may notice delayed audio at 250 ms or higher. You may sound robotic if your packet loss rate is over 10%. If the problem persists, disconnect and try again.")
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
        }

        // Privacy.
        Column {
            visible: root.tab === "privacy"
            x: 16
            width: parent.width - 32
            topPadding: 16
            bottomPadding: 16
            spacing: 12
            Row {
                spacing: 12
                Rectangle {
                    width: 40
                    height: 40
                    radius: 20
                    color: Theme.talkTint
                    Icon {
                        anchors.centerIn: parent
                        name: "lock"
                        size: Theme.iconLg
                        color: Theme.successInk
                    }
                }
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    Text {
                        textFormat: Text.PlainText
                        text: qsTr("End-to-end encrypted")
                        color: Theme.ink1
                        font: Theme.type.captionStrong
                    }
                    Text {
                        textFormat: Text.PlainText
                        text: qsTr("Your voice connection is secured with DTLS-SRTP encryption.")
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                }
            }
        }

        // Footer.
        Item {
            width: parent.width
            height: 48
            Rectangle { width: parent.width; height: 1; color: Theme.line }
            LedLabel {
                x: 16
                anchors.verticalCenter: parent.verticalCenter
                text: root.state === "connected" ? qsTr("Encrypted") : root.stateLabel(root.state)
                ledState: root.state === "connected" ? "talk" : "idle"
            }
            KitButton {
                visible: root.stats !== null
                anchors.right: parent.right
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                variant: "ghost"
                text: copied.running ? qsTr("Copied") : qsTr("Copy stats")
                onClicked: {
                    clipboard.copyText(JSON.stringify({ channel: Voice.channelName, state: root.state, stats: root.stats }, null, 2))
                    copied.restart()
                }
                Timer { id: copied; interval: 2000 }
            }
        }
    }
}
