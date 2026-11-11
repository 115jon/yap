import QtQuick
import com.yap.native
import Yap.Ui

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
    // What they are playing (in the custom status's place), and its icon.
    required property string game
    required property string gameIcon
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
    // The display-name style (JSON), or empty.
    required property string nameStyle
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
        Behavior on color { ColorAnimation { duration: Theme.dur1 } }

        HoverHandler { id: hover; enabled: !root.label; cursorShape: Qt.PointingHandCursor }

        // Made only for a member with a nameplate (performance pass 7).
        Loader {
            anchors.fill: parent
            active: root.plate.length > 0
            sourceComponent: NameplateLayer {
                plate: root.plate
                playing: hover.hovered
            }
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
        // Resting on the row loads the profile for its popout (banner and
        // effect ready as it opens).
        HoverHandler {
            enabled: !root.label
            onHoveredChanged: ShellState.prefetchProfile(hovered ? root.userId : "", root.avatar)
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
                // MemberList.tsx UserDisplayName: the owner's style, else the
                // role colour (online, no plate, no style), 2.8:1 on a plate.
                DisplayName {
                    id: nameText
                    playing: hover.hovered
                    width: Math.min(implicitWidth, parent.width - (crown.visible ? crown.width + 6 : 0)
                        - (devices.visible ? devices.implicitWidth + 6 : 0))
                    text: root.name
                    nameStyle: root.nameStyle
                    color: root.roleColor.length > 0 && root.plate.length === 0 && root.nameStyle.length === 0 ? root.roleInk
                        : hover.hovered || root.plate.length > 0 ? Theme.ink1 : Theme.ink2
                    font: Theme.type.row
                    ground: root.ground
                    minContrast: root.plate.length > 0 ? 2.8 : 3
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
                    LazyPopup {
                        // Made while hovered.
                        when: crownHover.hovered
                        sourceComponent: Tooltip {
                            visible: crownHover.hovered
                            text: qsTr("Server owner")
                        }
                    }
                }
            }
            // What they play takes the custom status's line (GameActivity.tsx).
            Loader {
                width: parent.width
                active: root.game.length > 0
                visible: active
                sourceComponent: GameLine {
                    game: root.game
                    icon: root.gameIcon
                }
            }
            Text {
                width: parent.width
                visible: root.game.length === 0 && root.customStatus.length > 0
                textFormat: Text.PlainText
                text: root.customStatus
                elide: Text.ElideRight
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
        // Offline: the row at half, as the web's opacity-50. The ground at
        // half over the row is the same picture as the row at half over
        // the ground, and needs no layer: a layer re-renders, then asks for
        // another window frame, on every frame of a decoration inside it.
        Rectangle {
            anchors.fill: parent
            z: 10
            radius: Theme.radius
            visible: !root.online && !hover.hovered
            color: Qt.rgba(root.ground.r, root.ground.g, root.ground.b, 0.5)
            Accessible.ignored: true
        }
    }
}
