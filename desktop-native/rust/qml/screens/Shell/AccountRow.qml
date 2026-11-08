import QtQuick
import com.yap.native

// The account row (UserPanel.tsx): 64 high, pl-3 pr-1, gap 4. A 40 px
// avatar with its presence LED in a square notch cut from the ply, the
// Bricolage 15/700 name over the status word (silkscreen in the LED's
// ink; the handle while hovered), then mic and deafen (quiet dock keys
// with their device carets) and settings. Mic and deafen work before and
// during a call; they are kept between calls. The carets open the device
// menus, which lead to Settings → Voice & video; the settings key opens
// Settings on Account. The avatar wears the speaking ring (rm-speak) while
// you talk.
Rectangle {
    id: root

    // ui/led.tsx LED_INK by presence (UserPanel.tsx PRESENCE_LED).
    readonly property string presence: Session.meStatus.length > 0 ? Session.meStatus : "online"
    readonly property color presenceLed: Theme.presenceLed(presence)
    readonly property color presenceInk: Theme.presenceInk(presence)
    readonly property string presenceWord: presence === "offline" ? qsTr("Invisible") : Theme.presenceWord(presence)

    // Over a nameplate the keys take its ink, as the web's controls do
    // (UserPanel.tsx nameplateInk/caretStyle): dark over a light accent,
    // else near-white; muted keeps the danger ink, lightened on a dark one.
    readonly property var plateInfo: Session.mePlate.length > 0 ? JSON.parse(Session.mePlate) : null
    readonly property bool onPlate: plateInfo !== null
    readonly property bool plateLight: onPlate && plateInfo.light === true
    readonly property color plateInk: !onPlate ? "transparent" : plateLight ? "#162033" : "#f8fafc"
    readonly property color plateInkQuiet: !onPlate ? "transparent"
        : plateLight ? Qt.rgba(0x16 / 255, 0x20 / 255, 0x33 / 255, 0.78)
        : Qt.rgba(0xf8 / 255, 0xfa / 255, 0xfc / 255, 0.84)
    readonly property color plateInkOff: !onPlate ? "transparent"
        : plateLight ? Theme.mutedState
        : Qt.tint("#ffffff", Qt.rgba(Theme.mutedState.r, Theme.mutedState.g, Theme.mutedState.b, 0.35))

    implicitHeight: 64
    color: Theme.ply1

    // Our nameplate behind the row (UserPanel.tsx); it plays while hovered.
    NameplateLayer {
        anchors.fill: parent
        anchors.topMargin: 1
        visible: Session.mePlate.length > 0
        plate: Session.mePlate
        playing: rowHover.hovered
        identityFade: true
        ground: Theme.ply1
    }
    HoverHandler { id: rowHover }

    Rectangle { width: parent.width; height: 1; color: Theme.line }

    // Identity: the avatar and the name column, one press target.
    Item {
        id: identity
        anchors.left: parent.left
        anchors.leftMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        anchors.right: keys.left
        anchors.rightMargin: 4
        height: 48
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("%1, %2, %3. View user account").arg(Session.meName).arg(Session.meUsername).arg(root.presenceWord)

        Rectangle {
            anchors.fill: parent
            radius: Theme.radius
            color: Theme.ink1
            opacity: identityTap.pressed ? 0.12 : identityHover.hovered ? 0.06 : 0
            Behavior on opacity { NumberAnimation { duration: Theme.dur1 } }
        }

        Avatar {
            id: avatar
            anchors.verticalCenter: parent.verticalCenter
            size: 40
            initials: Session.meId.length > 0 ? Session.meName : "?"
            source: Session.meAvatar
            deco: Session.meDeco
            // rm-speak: a 2 px talk ring at -2 px, opacity only.
            Rectangle {
                anchors.fill: parent
                anchors.margins: -2
                color: "transparent"
                radius: Theme.radius
                border.width: 2
                border.color: Theme.talk
                opacity: Voice.speaking && !Voice.muted ? 1 : 0
                Behavior on opacity { NumberAnimation { duration: Voice.speaking ? Theme.speakOn : Theme.speakOff } }
            }
        }
        // Presence: an 8 px LED in a 14 px square cut out of the panel ply.
        Rectangle {
            x: avatar.x + avatar.width - 12
            y: avatar.y + avatar.height - 12
            width: 14
            height: 14
            radius: Theme.radius
            color: Theme.ply1
            Led {
                anchors.centerIn: parent
                size: 8
                lit: root.presence !== "offline"
                color: root.presenceLed
            }
        }

        Item {
            anchors.left: avatar.right
            anchors.leftMargin: 8
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            height: 36
            clip: true

            DisplayName {
                id: nameText
                playing: rowHover.hovered
                width: Math.min(implicitWidth, parent.width)
                text: Session.meName
                nameStyle: Session.meNameStyle
                color: Theme.ink1
                font: Theme.type.author
                ground: Theme.ply1
            }
            // The status line, swapped for the handle while hovered
            // (a transform over dur-3).
            Item {
                anchors.top: nameText.bottom
                width: parent.width
                height: 16
                clip: true
                Column {
                    y: identityHover.hovered ? -16 : 0
                    width: parent.width
                    Behavior on y { NumberAnimation { duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
                    // What we play, as others see it, before the custom
                    // status (UserPanel.tsx).
                    Loader {
                        width: parent.width
                        height: 16
                        active: Session.meGame.length > 0
                        visible: active
                        sourceComponent: GameLine {
                            game: Session.meGame
                            icon: Session.meGameIcon
                            ink: Theme.ink2
                            strongInk: Theme.ink1
                        }
                    }
                    Text {
                        width: parent.width
                        height: 16
                        visible: Session.meGame.length === 0
                        verticalAlignment: Text.AlignVCenter
                        textFormat: Text.PlainText
                        text: Session.meCustomStatus.length > 0 ? Session.meCustomStatus : root.presenceWord
                        elide: Text.ElideRight
                        color: Session.meCustomStatus.length > 0 ? Theme.ink2 : root.presenceInk
                        font: Session.meCustomStatus.length > 0 ? Theme.type.caption : Theme.type.silk
                    }
                    Text {
                        width: parent.width
                        height: 16
                        verticalAlignment: Text.AlignVCenter
                        textFormat: Text.PlainText
                        text: Session.meUsername
                        elide: Text.ElideRight
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                }
            }
        }

        HoverHandler { id: identityHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { id: identityTap; onTapped: account.load()?.openAbove(identity) }
        LazyPopup {
            when: identityHover.hovered && !(account.item?.visible ?? false)
            sourceComponent: Tooltip {
                visible: identityHover.hovered && !(account.item?.visible ?? false)
                readonly property string line: Session.meGame.length > 0 ? qsTr("Playing %1").arg(Session.meGame) : Session.meCustomStatus
                text: qsTr("View profile · %1").arg(line.length > 0 ? root.presenceWord + ": " + line : root.presenceWord)
            }
        }

        LazyPopup { id: account; sourceComponent: AccountPopout {} }
    }

    Row {
        id: keys
        anchors.right: parent.right
        anchors.rightMargin: 4
        anchors.verticalCenter: parent.verticalCenter

        IconKey {
            iconName: "mic"
            plateInk: Voice.muted ? root.plateInkOff : root.plateInk
            tone: Voice.muted ? "off" : "ghost"
            tip: Voice.muted ? qsTr("Unmute") : qsTr("Mute")
            onClicked: Voice.requestMute(!Voice.muted)
        }
        DeviceCaret {
            plateInk: root.plateInk
            plateMuted: root.plateInkQuiet
            label: qsTr("Input settings")
            menu: inputMenu
        }
        IconKey {
            iconName: "deafen"
            plateInk: Voice.deafened ? root.plateInkOff : root.plateInk
            tone: Voice.deafened ? "off" : "ghost"
            tip: Voice.deafened ? qsTr("Undeafen") : qsTr("Deafen")
            onClicked: Voice.requestDeafen(!Voice.deafened)
        }
        DeviceCaret {
            plateInk: root.plateInk
            plateMuted: root.plateInkQuiet
            label: qsTr("Output settings")
            menu: outputMenu
        }
        IconKey {
            iconName: "settings"
            plateInk: root.plateInk
            tip: qsTr("User settings")
            onClicked: SettingsNav.show("account", "")
        }
    }

    LazyPopup {
        id: inputMenu
        sourceComponent: DeviceMenu {
            mode: "input"
            onVoiceSettings: SettingsNav.show("voice", "microphone")
        }
    }
    LazyPopup {
        id: outputMenu
        sourceComponent: DeviceMenu {
            mode: "output"
            onVoiceSettings: SettingsNav.show("voice", "speaker")
        }
    }
}
