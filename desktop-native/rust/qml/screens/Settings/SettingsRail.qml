pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// The live preview rails (rails.tsx): the four pages where you judge a
// feel rather than flip a fact. Each is a legend plate.
//   account        "How others see you": the profile card others open
//   voice          "What others get": the mic test and what goes out
//   appearance     "Preview": a slice of chat in the current theme
//   notifications  "Next mention": the next alert, and its chime
Loader {
    id: root

    property string panelId

    sourceComponent: panelId === "account" ? profileRail
        : panelId === "voice" ? voiceRail
        : panelId === "appearance" ? appearanceRail
        : notificationsRail

    Component {
        id: profileRail
        LegendPlate {
            legend: qsTr("How others see you")
            note: qsTr("Your profile card, as it looks when someone opens it.")
            ProfileCard {
                // rails.tsx: the card as it floats, shadow included.
                floating: true
                width: Math.min(parent.width, implicitWidth)
                name: Settings.name
                username: Settings.username
                avatar: Settings.avatar
                initials: Settings.initials
                pronouns: Settings.pronouns
                presence: Settings.status
                customStatus: Settings.customStatus
                banner: Settings.banner
                bannerColor: Settings.bannerColor
                surfaceColor: Settings.backgroundColor
                accentColor: Settings.accentColor
                // The card others open wears your collectibles and name style.
                deco: Settings.avatar.length > 0 ? Session.meDeco : ""
                effect: Session.meEffect
                frame: Session.meFrame
                nameStyle: Session.meNameStyle
                badges: Settings.badges
                Accessible.role: Accessible.Graphic
                Accessible.name: qsTr("Profile preview for %1").arg(Settings.name)
                AboutMe { text: Settings.bio }
            }
        }
    }

    Component {
        id: voiceRail
        LegendPlate {
            id: plate
            readonly property var items: [
                [qsTr("Microphone"), Audio.inputLabel.length > 0 ? Audio.inputLabel.replace(/^Default - /, "") : qsTr("Default microphone")],
                [qsTr("Input profile"), Audio.inputProfile === "standard" ? qsTr("Voice isolation") : Audio.inputProfile === "studio" ? qsTr("Studio") : qsTr("Custom")],
                [qsTr("Sends"), Audio.autoSensitivity ? qsTr("When you talk (automatic)") : qsTr("Above %1 dB").arg(Math.round(Audio.sensitivity))],
                [qsTr("Noise suppression"), Audio.noiseReduction === "rnnoise" ? qsTr("RNNoise") : Audio.noiseSuppression ? qsTr("WebRTC") : qsTr("Off")],
                [qsTr("Echo cancellation"), Audio.echoCancellation ? qsTr("On") : qsTr("Off")],
                [qsTr("Audio"), Audio.highFidelity ? qsTr("High fidelity") : qsTr("Standard")],
                [qsTr("Camera"), SettingsNav.qualityName(Audio.cameraQuality)],
                [qsTr("Background"), qsTr("None")],
            ]
            legend: qsTr("What others get")
            Column {
                width: parent.width
                spacing: 12
                readonly property string rowId: "mic-test"
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: SettingsNav.desc("mic-test")
                    wrapMode: Text.Wrap
                    color: Theme.ink2
                    font: Theme.type.description
                }
                // MicTestWidget: the chosen mic through the call's processing,
                // played back on the chosen output, with its level. It stops
                // when the page goes (this rail is unloaded) or Settings
                // closes; a call keeps sending its own mic meanwhile.
                Component.onDestruction: Audio.stopMicTest()
                Connections {
                    target: SettingsNav
                    function onOpenChanged() {
                        if (!SettingsNav.open)
                            Audio.stopMicTest()
                    }
                }
                // The test's level, read while it runs (VoicePanel reads the
                // call's while it shows).
                Timer {
                    interval: 33
                    repeat: true
                    running: Audio.micTesting
                    onTriggered: Audio.sample()
                }
                // The meter beside the key when it fits at 160 px or more,
                // else on the next line (the web's flex-wrap, min-w-40).
                Flow {
                    id: testRow
                    readonly property real beside: width - testKey.width - spacing
                    width: parent.width
                    spacing: 12
                    KitButton {
                        id: testKey
                        size: "lg"
                        variant: Audio.micTesting ? "destructive" : "secondary"
                        iconName: Audio.micTesting ? "stop" : "mic"
                        text: Audio.micTesting ? qsTr("Stop test") : qsTr("Test mic")
                        Accessible.checkable: true
                        Accessible.checked: Audio.micTesting
                        onClicked: Audio.micTesting ? Audio.stopMicTest() : Audio.startMicTest()
                    }
                    LevelMeter {
                        width: testRow.beside >= 160 ? testRow.beside : testRow.width
                        // As tall as the key beside it, so its cells sit on the key's middle.
                        height: testRow.beside >= 160 ? testKey.height : implicitHeight
                        value: Audio.autoSensitivity ? 69.5 : Audio.sensitivity + 100
                        level: Audio.level
                        metering: Audio.levelLive
                    }
                }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: Audio.micTesting ? qsTr("You're hearing yourself as others will.")
                        : !Audio.levelLive ? qsTr("Test your mic, or join a voice channel to see your level.")
                        : Voice.muted ? qsTr("Unmute to see your level.")
                        : qsTr("Talk to see your level.")
                    wrapMode: Text.Wrap
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
            Column {
                width: parent.width
                Rectangle { width: parent.width; height: 1; color: Theme.line }
                Repeater {
                    model: plate.items
                    delegate: Item {
                        id: item
                        required property var modelData
                        width: parent.width
                        height: 33
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: item.modelData[0]
                            color: Theme.ink2
                            font: Theme.type.caption
                        }
                        Text {
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            width: Math.min(implicitWidth, parent.width - 120)
                            horizontalAlignment: Text.AlignRight
                            textFormat: Text.PlainText
                            text: item.modelData[1]
                            elide: Text.ElideRight
                            color: Theme.ink1
                            font: Theme.type.captionStrong
                        }
                        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line }
                    }
                }
            }
        }
    }

    Component {
        id: appearanceRail
        LegendPlate {
            id: preview
            readonly property string name: SettingsNav.themeName(Settings.theme)
            readonly property var sample: [
                // rails.tsx SAMPLE_CHAT: chart-3 (idle), accent-ink, chart-2 (talk).
                { who: qsTr("Ada"), color: Theme.idle, text: qsTr("Is the voice room still open? I'm joining in five.") },
                { who: qsTr("You"), color: Theme.accentInk, text: qsTr("Come through. Noise suppression is on, the fan's back.") },
                { who: qsTr("Grace"), color: Theme.talk, text: qsTr("Sharing my screen in a sec.") },
            ]
            legend: qsTr("Preview")
            note: qsTr("%1. Changes apply as you pick.").arg(name)
            Rectangle {
                width: parent.width
                height: chat.implicitHeight + 24
                radius: Theme.radius
                color: Theme.ply0
                border.width: 1
                border.color: Theme.line
                Accessible.role: Accessible.Graphic
                Accessible.name: qsTr("Sample chat in the %1 theme").arg(preview.name)
                Column {
                    id: chat
                    x: 16
                    y: 12
                    width: parent.width - 32
                    spacing: 12
                    Repeater {
                        model: preview.sample
                        delegate: Row {
                            id: line
                            required property var modelData
                            width: chat.width
                            spacing: 12
                            Rectangle {
                                y: 2
                                width: 32
                                height: 32
                                radius: Theme.radius
                                color: line.modelData.color
                                opacity: 0.85
                            }
                            Column {
                                width: parent.width - 44
                                Text {
                                    textFormat: Text.PlainText
                                    text: line.modelData.who
                                    // rm-role-name: darkened on light grounds.
                                    color: Theme.roleName(line.modelData.color)
                                    font.family: "Yap Geist SemiBold"
                                    font.pixelSize: 13
                                    font.weight: Font.DemiBold
                                }
                                Text {
                                    width: parent.width
                                    textFormat: Text.PlainText
                                    text: line.modelData.text
                                    wrapMode: Text.Wrap
                                    color: Theme.ink1
                                    font: Theme.type.description
                                }
                            }
                        }
                    }
                    Rectangle {
                        width: parent.width
                        height: 32
                        radius: Theme.radius
                        color: Theme.ply2
                        border.width: 1
                        border.color: Theme.lineField
                        Text {
                            x: 12
                            anchors.verticalCenter: parent.verticalCenter
                            text: qsTr("Message #general")
                            color: Theme.ink2
                            font: Theme.type.description
                        }
                    }
                }
            }
        }
    }

    Component {
        id: notificationsRail
        LegendPlate {
            id: alert
            readonly property bool soundOn: Desktop.sounds && Desktop.soundNotifications
            readonly property bool alertsOff: !Desktop.notifications
            legend: qsTr("Next mention")
            note: alertsOff ? qsTr("Desktop notifications are off. Mentions still show unread badges.")
                : qsTr("Shows as a native notification for mentions, replies and DMs.")
            Rectangle {
                width: parent.width
                height: 64
                radius: Theme.radius
                color: Theme.ply0
                border.width: 1
                border.color: Theme.lineStrong
                opacity: alert.alertsOff ? 0.5 : 1
                Behavior on opacity { NumberAnimation { duration: 200 } }
                Rectangle {
                    x: 12
                    y: 12
                    width: 36
                    height: 36
                    radius: Theme.radius
                    color: Theme.primaryFill
                    Icon {
                        anchors.centerIn: parent
                        name: "bell"
                        size: Theme.iconSm
                        color: Theme.primaryFillInk
                    }
                }
                Column {
                    x: 60
                    y: 10
                    width: parent.width - 72
                    Text { text: qsTr("Yap · now"); color: Theme.ink2; font: Theme.type.caption }
                    Text { text: qsTr("Ada in #design"); color: Theme.ink1; font: Theme.type.rowUnread }
                    Text {
                        width: parent.width
                        text: qsTr("@you is the voice room still open?")
                        elide: Text.ElideRight
                        color: Theme.ink2
                        font: Theme.type.description
                    }
                }
            }
            Row {
                spacing: 8
                Icon {
                    anchors.verticalCenter: parent.verticalCenter
                    name: alert.soundOn ? "voiceChannel" : "speakerOff"
                    size: Theme.iconSm
                    color: Theme.ink2
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: alert.soundOn ? qsTr("Plays a chime at %1% volume.").arg(Desktop.soundVolume)
                        : Desktop.sounds ? qsTr("No chime: notification sounds are off.")
                        : qsTr("No chime: sound effects are off.")
                    color: Theme.ink2
                    font: Theme.type.caption
                }
            }
        }
    }

    // About me (ProfileSections): the label over the bio.
    component AboutMe: Column {
        property alias text: bio.text
        visible: bio.text.length > 0
        width: parent.width
        spacing: 8
        Text {
            text: qsTr("About me")
            color: Theme.ink2
            font: Theme.type.captionStrong
            Accessible.role: Accessible.Heading
        }
        Text {
            id: bio
            textFormat: Text.PlainText
            width: parent.width
            wrapMode: Text.Wrap
            maximumLineCount: 12
            elide: Text.ElideRight
            color: Theme.ink1
            font: Theme.type.description
        }
    }
}
