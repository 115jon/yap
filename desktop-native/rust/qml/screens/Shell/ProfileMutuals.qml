pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// ProfileSections' "Mutual servers" and "Mutual friends" (ProfileCard.tsx):
// each a 12/500 ink-2 heading with its count in ink-3 Num, 8 over a wrap of
// chips 6 apart, up to `limit` (the popover 6, the full profile 8). A chip
// is 24 high on tint-1 (2 px corners, 2 px in on the left, 10 on the
// right): a 20 px tint-3 square with the server's icon or the friend's
// avatar (round under a decoration), else the initial at 11/800, then the
// name at 13 ink-1, truncated.
Column {
    id: root

    // `{count, items}` JSON from the core, or empty.
    property string servers: ""
    property string friends: ""
    property int limit: 6

    readonly property var serverInfo: servers.length > 0 ? JSON.parse(servers) : null
    readonly property var friendInfo: friends.length > 0 ? JSON.parse(friends) : null

    width: parent ? parent.width : 0
    spacing: 16
    visible: (serverInfo !== null && serverInfo.count > 0) || (friendInfo !== null && friendInfo.count > 0)

    Section {
        title: qsTr("Mutual servers")
        info: root.serverInfo
        round: false
    }
    Section {
        title: qsTr("Mutual friends")
        info: root.friendInfo
        round: true
    }

    component Section: Column {
        id: section
        required property string title
        required property var info
        required property bool round
        visible: info !== null && info.count > 0
        width: root.width
        spacing: 8
        Row {
            spacing: 6
            Text {
                id: heading
                text: section.title
                color: Theme.ink2
                font: Theme.type.captionStrong
                Accessible.role: Accessible.Heading
            }
            Text {
                anchors.baseline: heading.baseline
                textFormat: Text.PlainText
                text: section.info ? String(section.info.count) : ""
                color: Theme.ink3
                font: Theme.type.numSm
            }
        }
        Flow {
            width: parent.width
            spacing: 6
            Repeater {
                model: section.info ? section.info.items.slice(0, root.limit) : []
                delegate: Rectangle {
                    id: chip
                    required property var modelData
                    readonly property string picture: modelData.icon ?? modelData.avatar ?? ""
                    readonly property bool decorated: section.round && (modelData.deco ?? "").length > 0
                    width: Math.min(chipRow.implicitWidth + 2 + 10, parent ? parent.width : 300)
                    height: 24
                    radius: Theme.radius
                    color: Theme.tint1
                    Accessible.role: Accessible.StaticText
                    Accessible.name: modelData.name
                    Row {
                        id: chipRow
                        x: 2
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 6
                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 20
                            height: 20
                            radius: chip.decorated ? 10 : Theme.radius
                            color: Theme.tint3
                            clip: true
                            Text {
                                anchors.centerIn: parent
                                visible: icon.status !== Image.Ready
                                textFormat: Text.PlainText
                                text: chip.modelData.initial ?? ""
                                color: Theme.ink1
                                font.family: "Yap Bricolage Key ExtraBold"
                                font.pixelSize: 11
                                font.weight: Font.ExtraBold
                            }
                            Image {
                                id: icon
                                anchors.fill: parent
                                source: chip.picture
                                sourceSize: Qt.size(40, 40)
                                asynchronous: true
                                fillMode: Image.PreserveAspectCrop
                            }
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            width: Math.min(implicitWidth, (chip.parent ? chip.parent.width : 300) - 2 - 20 - 6 - 10)
                            elide: Text.ElideRight
                            textFormat: Text.PlainText
                            text: chip.modelData.name
                            color: Theme.ink1
                            font: Theme.type.description
                        }
                    }
                }
            }
        }
    }
}
