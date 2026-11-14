pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// Favourite servers (widgets/Servers.tsx): up to 4 of the owner's servers
// with their size, and whether the viewer is in each too. S is a row of
// tiles with the first one named; M lists them; L lists them in two
// columns (one when narrow, then two at most).
WidgetPlate {
    id: root

    required property var widget
    required property var boardData
    required property var ctx

    readonly property var servers: root.boardData?.servers ?? []
    readonly property real bodyWidth: root.body.width

    kind: widget.type
    size: widget.size
    draft: widget.draft ?? false
    readout: size === "S" && servers.length > 0 ? String(servers.length) : ""

    // A server's icon, or its initial on tint-3.
    component ServerTile: Item {
        id: tile
        property var server: ({})
        width: 40
        height: 40
        readonly property string icon: server?.icon_url ?? ""

        WidgetImage {
            visible: tile.icon.length > 0
            anchors.fill: parent
            source: tile.icon
            decodeWidth: 80
        }
        Rectangle {
            visible: tile.icon.length === 0
            anchors.fill: parent
            radius: Theme.radius
            color: Theme.tint3
            Text {
                anchors.centerIn: parent
                textFormat: Text.PlainText
                text: String(tile.server?.name ?? "").slice(0, 1).toUpperCase()
                color: Theme.ink1
                font: Theme.type.railInitial
            }
        }
    }

    WidgetEmpty {
        visible: root.servers.length === 0
        icon: "server"
        title: root.ctx?.isOwner ? qsTr("No servers picked") : qsTr("No servers to show")
        hint: root.ctx?.isOwner && root.size !== "S" ? qsTr("Pick up to 4 in Edit board.") : ""
    }

    // S: the tiles, then the first server's name and size at the foot.
    Item {
        visible: root.servers.length > 0 && root.size === "S"
        anchors.fill: parent

        Row {
            spacing: 8
            Accessible.role: Accessible.List
            Accessible.name: qsTr("Servers")
            Repeater {
                model: root.size === "S" ? root.servers.slice(0, root.bodyWidth < 200 ? 3 : 4) : []
                delegate: ServerTile {
                    id: sTile
                    required property var modelData
                    server: modelData
                    Accessible.role: Accessible.ListItem
                    Accessible.name: modelData.name ?? ""
                    HoverHandler { id: tileHover }
                    Tooltip {
                        visible: tileHover.hovered
                        text: sTile.modelData.name ?? ""
                    }
                }
            }
        }
        Column {
            anchors.bottom: parent.bottom
            width: parent.width
            spacing: 2
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: root.servers[0]?.name ?? ""
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.labelStrong
            }
            Row {
                spacing: 0
                Text {
                    textFormat: Text.PlainText
                    text: String(root.servers[0]?.member_count ?? 0)
                    color: Theme.ink2
                    font: Theme.type.num
                    anchors.baseline: membersLabel.baseline
                }
                Text {
                    id: membersLabel
                    textFormat: Text.PlainText
                    text: " " + qsTr("members")
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
        }
    }

    // M and L: one row per server.
    Grid {
        id: list
        visible: root.servers.length > 0 && root.size !== "S"
        width: parent.width
        readonly property bool twoCols: root.size === "L" && root.bodyWidth >= 420
        columns: twoCols ? 2 : 1
        columnSpacing: 16
        rowSpacing: 12
        Accessible.role: Accessible.List
        Accessible.name: qsTr("Servers")

        Repeater {
            model: root.size === "S" ? []
                : root.servers.slice(0, root.size === "L" && !list.twoCols ? 2 : 4)
            delegate: Row {
                id: row
                required property var modelData
                width: list.twoCols ? Math.floor((list.width - list.columnSpacing) / 2) : list.width
                spacing: 10
                Accessible.role: Accessible.ListItem
                Accessible.name: modelData.name ?? ""

                ServerTile {
                    server: row.modelData
                    anchors.verticalCenter: parent.verticalCenter
                }
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    width: row.width - 40 - row.spacing
                    spacing: 2
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: row.modelData.name ?? ""
                        elide: Text.ElideRight
                        color: Theme.ink1
                        font: Theme.type.labelStrong
                    }
                    Row {
                        width: parent.width
                        spacing: 4
                        clip: true
                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: "members"
                            size: Theme.iconXs
                            color: Theme.ink3
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: String(row.modelData.member_count ?? 0)
                            color: Theme.ink2
                            font: Theme.type.num
                        }
                        Text {
                            visible: (row.modelData.viewer_is_member ?? false) && root.bodyWidth >= 200
                            anchors.verticalCenter: parent.verticalCenter
                            width: Math.min(implicitWidth, parent.width - x)
                            textFormat: Text.PlainText
                            text: "· " + qsTr("You're in it")
                            elide: Text.ElideRight
                            color: Theme.ink3
                            font: Theme.type.caption
                        }
                    }
                }
            }
        }
    }
}
