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
        }
        return out
    }

    color: Theme.ply1

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
        anchors.bottom: footer.top
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
            sourceComponent: cell.modelData.kind === "label" ? labelRow : channelRow

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
                    readonly property bool active: cell.modelData.id === root.client.activeChannelId
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
