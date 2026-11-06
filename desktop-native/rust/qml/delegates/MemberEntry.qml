import QtQuick
import com.yap.native

// One member panel row (MemberList.tsx). A label is the branch label: 24
// high, silkscreen ink-3, the count at the end, 16 above (8 for the first).
// A member is a 40 px row, pl-2.5, gap 12: a 32 px avatar with presence (or
// typing dots) cut out of ply-0, the name in their highest role's colour
// while online, where they are signed in (UserPlatformIndicators.tsx) and a
// crown for the owner, and the custom status under it.
// Offline rows sit at half opacity until hovered. Someone in your voice
// channel gets their talk LED in the row's left gutter. A nameplate
// covers the row behind them (its still; the webm plays while hovered),
// edged in its palette, and the name drops its role colour.
Item {
    id: root

    required property int index
    required property string kind
    required property string userId
    required property string name
    required property int count
    required property string avatar
    required property string initials
    required property string presence
    required property string customStatus
    required property string roleColor
    // Light themes darken it (styles.css .rm-role-name).
    readonly property color roleInk: roleColor.length > 0 ? Theme.roleName(roleColor) : Theme.ink2
    required property bool online
    required property bool owner
    required property bool me
    required property bool typing
    required property bool inCall
    required property bool speaking
    // The avatar decoration (APNG) and the nameplate JSON, or empty.
    required property string deco
    required property string plate
    required property string platforms
    property color ground: Theme.ply0
    signal profileRequested(string userId, Item anchor)
    // A right click at `at` (in the row): the member's menu.
    signal menuRequested(string userId, string name, Item anchor, point at)

    readonly property bool label: kind === "label"
    readonly property int gap: label ? (index === 0 ? 8 : 16) : 0

    height: label ? 24 + gap : 40
    Accessible.role: label ? Accessible.Heading : Accessible.Button
    Accessible.name: label ? qsTr("%1, %2").arg(name).arg(count)
        : qsTr("%1 (%2)").arg(name).arg(online ? qsTr("Online") : qsTr("Offline"))
          + (typing ? qsTr(", typing") : "")

    // Label
    Item {
        visible: root.label
        y: root.gap
        width: parent.width
        height: 24
        Text {
            anchors.left: parent.left
            anchors.leftMargin: 8
            anchors.right: number.left
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: root.name
            elide: Text.ElideRight
            color: Theme.ink3
            font: Theme.type.silk
        }
        Text {
            id: number
            anchors.right: parent.right
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: root.count
            color: Theme.ink3
            font: Theme.type.numSm
        }
    }

    // Member
    Rectangle {
        id: member
        visible: !root.label
        anchors.fill: parent
        radius: Theme.radius
        color: hover.hovered ? Theme.tint1 : "transparent"
        opacity: root.online || hover.hovered ? 1 : 0.5
        Behavior on color { ColorAnimation { duration: Theme.dur1 } }

        HoverHandler { id: hover; enabled: !root.label; cursorShape: Qt.PointingHandCursor }

        NameplateLayer {
            anchors.fill: parent
            visible: root.plate.length > 0
            plate: root.plate
            playing: hover.hovered
        }
        Rectangle {
            anchors.fill: parent
            visible: root.plate.length > 0
            radius: Theme.radius
            color: "transparent"
            border.width: 1
            border.color: visible ? JSON.parse(root.plate).border : "transparent"
        }
        TapHandler {
            enabled: !root.label
            onTapped: root.profileRequested(root.userId, face)
        }
        TapHandler {
            enabled: !root.label
            acceptedButtons: Qt.RightButton
            onTapped: eventPoint => root.menuRequested(root.userId, root.name, member, eventPoint.position)
        }

        Led {
            visible: root.inCall
            x: 2
            anchors.verticalCenter: parent.verticalCenter
            lit: root.speaking
            color: Theme.talk
        }

        PresenceAvatar {
            id: face
            x: 10
            anchors.verticalCenter: parent.verticalCenter
            size: 32
            initials: root.initials
            source: root.avatar
            presence: root.presence
            typing: root.typing
            ground: root.ground
            deco: root.deco
        }

        Column {
            anchors.left: face.right
            anchors.leftMargin: 12
            anchors.right: parent.right
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            Row {
                width: parent.width
                spacing: 6
                Text {
                    id: nameText
                    width: Math.min(implicitWidth, parent.width - (crown.visible ? crown.width + 6 : 0)
                        - (devices.visible ? devices.implicitWidth + 6 : 0))
                    textFormat: Text.PlainText
                    text: root.name
                    elide: Text.ElideRight
                    color: root.roleColor.length > 0 && root.plate.length === 0 ? root.roleInk
                        : hover.hovered || root.plate.length > 0 ? Theme.ink1 : Theme.ink2
                    font: Theme.type.row
                }
                PlatformIcons {
                    id: devices
                    anchors.verticalCenter: nameText.verticalCenter
                    platforms: root.platforms
                    presence: root.presence
                }
                Icon {
                    id: crown
                    anchors.verticalCenter: nameText.verticalCenter
                    visible: root.owner
                    name: "crown"
                    size: Theme.iconXs
                    color: Theme.warningInk
                    HoverHandler { id: crownHover }
                    Tooltip {
                        visible: crownHover.hovered
                        text: qsTr("Server owner")
                    }
                }
            }
            Text {
                width: parent.width
                visible: root.customStatus.length > 0
                textFormat: Text.PlainText
                text: root.customStatus
                elide: Text.ElideRight
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
    }
}
