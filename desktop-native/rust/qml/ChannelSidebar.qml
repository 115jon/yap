import QtQuick
import QtQuick.Shapes

// The tree panel: server name, then channels under silkscreen category
// labels, then the gateway LED.
Rectangle {
    id: root

    required property var client

    readonly property var servers: client.serversJson.length > 0 ? JSON.parse(client.serversJson) : []
    readonly property var server: servers.find(s => s.id === client.activeServerId) || null
    readonly property var rows: {
        const channels = client.channelsJson.length > 0 ? JSON.parse(client.channelsJson) : []
        const out = []
        let category = null
        for (const c of channels) {
            if (c.category !== category) {
                category = c.category
                if (category)
                    out.push({ kind: "label", name: category })
            }
            out.push({ kind: "channel", id: c.id, name: c.name, type: c.type })
            // The call we are in: its roster hangs under the channel.
            if (c.id === client.voiceChannelId)
                for (const m of voiceRoster)
                    out.push({ kind: "member", id: m.id, name: m.name, muted: m.muted, speaking: m.speaking, me: m.me })
        }
        return out
    }
    readonly property var voiceRoster: client.voiceRosterJson.length > 0 ? JSON.parse(client.voiceRosterJson) : []
    readonly property bool inCall: client.voiceChannelId.length > 0
    readonly property bool callLive: inCall && client.voiceStatus === "Connected"
    readonly property string callChannelName: {
        const channels = client.channelsJson.length > 0 ? JSON.parse(client.channelsJson) : []
        const c = channels.find(ch => ch.id === client.voiceChannelId)
        return c ? c.name : ""
    }

    color: Theme.ply1

    component CallKey: Rectangle {
        id: key
        property string label
        property bool lit: false
        property color litColor: Theme.ink1
        signal clicked()
        height: 28
        width: keyText.implicitWidth + 24
        radius: Theme.radius
        color: lit ? litColor : keyHover.hovered ? Theme.ply4 : Theme.ply3
        Text {
            id: keyText
            anchors.centerIn: parent
            text: key.label
            color: key.lit ? Theme.ply0 : Theme.ink1
            font.family: Theme.mono
            font.pixelSize: 11
            font.weight: Font.DemiBold
            font.letterSpacing: 0.8
        }
        HoverHandler { id: keyHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: key.clicked() }
    }


    Rectangle {
        anchors.right: parent.right
        width: 1
        height: parent.height
        color: Theme.ply2
    }

    Item {
        id: header
        width: parent.width
        height: Theme.header

        Text {
            anchors.left: parent.left
            anchors.leftMargin: 16
            anchors.right: parent.right
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            text: root.server ? root.server.name : ""
            elide: Text.ElideRight
            color: Theme.ink1
            font.family: Theme.display
            font.pixelSize: 18
            font.weight: Font.ExtraBold
            font.letterSpacing: -0.4
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.ply2
        }
    }

    ListView {
        id: list
        anchors.top: header.bottom
        anchors.topMargin: 8
        anchors.bottom: callPanel.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: 8
        anchors.rightMargin: 8
        clip: true
        model: root.rows
        boundsBehavior: Flickable.StopAtBounds

        delegate: Loader {
            id: cell
            required property var modelData
            required property int index
            width: list.width
            sourceComponent: cell.modelData.kind === "label" ? labelRow : cell.modelData.kind === "member" ? memberRow : channelRow

            Component {
                id: memberRow
                Item {
                    height: 28
                    // Speaking LED: lit green while they talk.
                    Rectangle {
                        id: led
                        x: 30
                        anchors.verticalCenter: parent.verticalCenter
                        width: 6; height: 6; radius: 3
                        color: cell.modelData.speaking ? Theme.talk : Theme.ply4
                        Behavior on color { ColorAnimation { duration: 90 } }
                    }
                    Text {
                        anchors.left: led.right
                        anchors.leftMargin: 10
                        anchors.right: muteTag.left
                        anchors.rightMargin: 6
                        anchors.verticalCenter: parent.verticalCenter
                        text: cell.modelData.name + (cell.modelData.me ? " (you)" : "")
                        elide: Text.ElideRight
                        color: cell.modelData.speaking ? Theme.ink1 : Theme.ink2
                        font.family: Theme.text
                        font.pixelSize: 13
                    }
                    Text {
                        id: muteTag
                        anchors.right: parent.right
                        anchors.rightMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        visible: cell.modelData.muted
                        text: "MUTED"
                        color: Theme.danger
                        font.family: Theme.mono
                        font.pixelSize: 10
                        font.letterSpacing: 0.8
                    }
                }
            }

            Component {
                id: labelRow
                Item {
                    height: cell.index === 0 ? 24 : 36
                    Text {
                        anchors.left: parent.left
                        anchors.leftMargin: 8
                        anchors.bottom: parent.bottom
                        anchors.bottomMargin: 6
                        text: cell.modelData.name.toUpperCase()
                        color: Theme.ink3
                        font.family: Theme.mono
                        font.pixelSize: 11
                        font.weight: Font.Medium
                        font.letterSpacing: 1
                    }
                }
            }

            Component {
                id: channelRow
                Rectangle {
                    id: row
                    readonly property bool active: cell.modelData.id === root.client.activeChannelId || cell.modelData.id === root.client.voiceChannelId
                    readonly property bool voice: cell.modelData.type === "voice"
                    height: Theme.treeRow
                    radius: Theme.radius
                    color: active ? Theme.ply3 : hover.hovered ? Theme.ply2 : "transparent"

                    Text {
                        id: glyph
                        visible: !row.voice
                        anchors.left: parent.left
                        anchors.leftMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        text: "#"
                        color: row.active ? Theme.ink2 : Theme.inkOff
                        font.family: Theme.mono
                        font.pixelSize: 15
                    }
                    // Speaker glyph for voice channels.
                    Shape {
                        visible: row.voice
                        x: 8
                        anchors.verticalCenter: parent.verticalCenter
                        width: 14
                        height: 14
                        preferredRendererType: Shape.CurveRenderer
                        ShapePath {
                            strokeWidth: 1.4
                            strokeColor: row.active ? Theme.ink2 : Theme.inkOff
                            fillColor: "transparent"
                            joinStyle: ShapePath.RoundJoin
                            capStyle: ShapePath.RoundCap
                            PathSvg { path: "M2 5 H5 L9 2 V12 L5 9 H2 Z M11.5 4.5 Q13.5 7 11.5 9.5" }
                        }
                    }
                    Text {
                        anchors.left: parent.left
                        anchors.leftMargin: 28
                        anchors.right: parent.right
                        anchors.rightMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        text: cell.modelData.name
                        elide: Text.ElideRight
                        color: row.active ? Theme.ink1 : hover.hovered ? Theme.ink2 : Theme.ink3
                        font.family: Theme.text
                        font.pixelSize: 14
                        font.weight: Font.Medium
                    }
                    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: root.client.selectChannel(cell.modelData.id) }
                }
            }
        }
    }

    // The call: status LED, channel, mute and leave keys.
    Rectangle {
        id: callPanel
        anchors.bottom: footer.top
        width: parent.width
        height: root.inCall ? 92 : 0
        visible: root.inCall
        color: Theme.ply2
        clip: true
        Rectangle { width: parent.width; height: 1; color: Theme.ply3 }

        Row {
            x: 16
            y: 14
            spacing: 8
            Rectangle {
                width: 6; height: 6; radius: 3
                anchors.verticalCenter: parent.verticalCenter
                color: root.callLive ? Theme.talk : Theme.idle
            }
            Text {
                text: "VOICE · " + (root.client.voiceStatus || "Connecting").toUpperCase()
                color: root.callLive ? Theme.talk : Theme.ink3
                font.family: Theme.mono
                font.pixelSize: 11
                font.weight: Font.Medium
                font.letterSpacing: 1
            }
        }
        Text {
            x: 16
            y: 32
            width: parent.width - 32
            text: root.callChannelName + (root.server ? " / " + root.server.name : "")
            elide: Text.ElideRight
            color: Theme.ink2
            font.family: Theme.text
            font.pixelSize: 13
        }

        Row {
            x: 16
            y: 54
            spacing: 6
            CallKey {
                label: root.client.voiceMuted ? "UNMUTE" : "MUTE"
                lit: root.client.voiceMuted
                litColor: Theme.danger
                onClicked: root.client.muteVoice(!root.client.voiceMuted)
            }
            CallKey {
                label: "LEAVE"
                onClicked: root.client.leaveVoice()
            }
        }
    }

    Rectangle {
        id: footer
        anchors.bottom: parent.bottom
        width: parent.width
        height: 40
        color: Theme.ply1
        Rectangle { width: parent.width; height: 1; color: Theme.ply2 }

        Row {
            anchors.left: parent.left
            anchors.leftMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            Rectangle {
                width: 6; height: 6; radius: 3
                anchors.verticalCenter: parent.verticalCenter
                color: root.client.connected ? Theme.talk : Theme.idle
            }
            Text {
                text: root.client.connected ? "GATEWAY · LIVE" : "GATEWAY · CONNECTING"
                color: Theme.ink3
                font.family: Theme.mono
                font.pixelSize: 11
                font.weight: Font.Medium
                font.letterSpacing: 1
            }
        }
    }
}
