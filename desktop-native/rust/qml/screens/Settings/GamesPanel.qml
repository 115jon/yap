pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// Registered games (this client's own page, Discord's "Registered games"):
// the detection switch, the games seen on this computer (newest first, with
// when each last ran and a Hide key), and "Add it" for a program detection
// does not know, picked from what has a window now. Kept on this device in
// `games.json`; what others see is Privacy & safety > Share my activity.
Column {
    id: root

    readonly property var games: JSON.parse(Games.games.length > 0 ? Games.games : "[]")
    readonly property var running: JSON.parse(Games.running.length > 0 ? Games.running : "[]")
    property bool picking: false
    readonly property var deadlock: JSON.parse(Games.deadlock.length > 0 ? Games.deadlock : "{}")

    spacing: 40

    // What the live stats are doing while Deadlock runs.
    function deadlockNote() {
        const status = root.deadlock.status
        if (status === "missing" || status === "stale")
            return qsTr("Deadlock is running without live stats. Next time, launch it from here or add the launch option in Steam.")
        const phases = {
            "menu": qsTr("In the menu"),
            "hideout": qsTr("In the Hideout"),
            "queue": qsTr("Looking for a match"),
            "match_intro": qsTr("Match starting"),
            "in_match": qsTr("In a match"),
            "post_match": qsTr("Match over"),
            "spectating": qsTr("Spectating")
        }
        const phase = phases[root.deadlock.phase] ?? qsTr("Reading the game's log")
        return qsTr("Live stats are on: %1.").arg(phase)
    }

    function lastPlayed(game) {
        if (game.playing)
            return qsTr("Playing now")
        if (!game.lastPlayed)
            return game.added ? qsTr("Added by you") : qsTr("Not played yet")
        const minutes = Math.floor((Date.now() - game.lastPlayed) / 60000)
        if (minutes < 1)
            return qsTr("Last played just now")
        if (minutes < 60)
            return qsTr("Last played %n min ago", "", minutes)
        if (minutes < 24 * 60)
            return qsTr("Last played %n h ago", "", Math.floor(minutes / 60))
        if (minutes < 7 * 24 * 60)
            return qsTr("Last played %n day(s) ago", "", Math.floor(minutes / (24 * 60)))
        return qsTr("Last played %1").arg(new Date(game.lastPlayed).toLocaleDateString(Qt.locale(), "MMM d, yyyy"))
    }

    SettingsCard {
        width: parent.width
        sectionId: "game-detection"
        SwitchSetting {
            rowId: "game-detection"
            checked: Games.detection
            onToggled: Games.enableDetection(!Games.detection)
        }
    }

    // Deadlock's live stats: the game writes console.log only when started
    // with -condebug, so offer that start and the launch option, and say so
    // gently while it runs without one.
    SettingsCard {
        id: deadlockCard
        width: parent.width
        sectionId: "deadlock-live"
        property bool copied: false

        SettingsRow {
            rowId: "deadlock-launch"
            layout: "inline"
            KitButton {
                variant: "secondary"
                size: "sm"
                iconName: "play"
                text: qsTr("Launch")
                Accessible.name: qsTr("Launch Deadlock with live stats")
                onClicked: Games.launchDeadlock()
            }
        }
        SettingsRow {
            rowId: "deadlock-launch-option"
            layout: "inline"
            Row {
                spacing: 8
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: optionText.implicitWidth + 16
                    height: 28
                    radius: Theme.radius
                    color: Theme.tint2
                    Text {
                        id: optionText
                        anchors.centerIn: parent
                        textFormat: Text.PlainText
                        text: root.deadlock.launchOption ?? "-condebug"
                        color: Theme.ink1
                        font: Theme.type.code
                    }
                }
                KitButton {
                    anchors.verticalCenter: parent.verticalCenter
                    size: "sm"
                    variant: "ghost"
                    iconName: deadlockCard.copied ? "check" : "copy"
                    text: deadlockCard.copied ? qsTr("Copied") : qsTr("Copy")
                    Accessible.name: qsTr("Copy the launch option")
                    onClicked: {
                        optionClipboard.copyText(optionText.text)
                        deadlockCard.copied = true
                        copiedTimer.restart()
                    }
                }
            }
        }
        SettingsRow {
            custom: true
            hideLabel: true
            layout: "block"
            visible: root.deadlock.running === true && root.deadlock.status !== "waiting"
            Text {
                width: parent.width
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                text: root.deadlockNote()
                color: root.deadlock.status === "live" ? Theme.accentInk : Theme.ink2
                font: Theme.type.description
            }
        }
        Timer {
            id: copiedTimer
            interval: 2000
            onTriggered: deadlockCard.copied = false
        }
        ClipboardText { id: optionClipboard }
    }

    SettingsCard {
        width: parent.width
        sectionId: "detected-games"
        SettingsRow {
            rowId: "detected-games"
            layout: "block"
            Column {
                width: parent.width
                spacing: 4

                Text {
                    visible: root.games.length === 0
                    width: parent.width
                    textFormat: Text.PlainText
                    text: Games.detection
                        ? qsTr("Nothing yet. Open a game and it shows up here.")
                        : qsTr("Detection is off.")
                    wrapMode: Text.Wrap
                    color: Theme.ink3
                    font: Theme.type.description
                }

                Repeater {
                    model: root.games
                    delegate: Rectangle {
                        id: gameRow
                        required property var modelData
                        width: parent.width
                        height: 52
                        radius: Theme.radius
                        color: rowHover.hovered ? Theme.tint1 : "transparent"
                        HoverHandler { id: rowHover }

                        Rectangle {
                            id: tile
                            x: 8
                            anchors.verticalCenter: parent.verticalCenter
                            width: 32
                            height: 32
                            radius: Theme.radius
                            color: Theme.tint2
                            clip: true
                            opacity: gameRow.modelData.hidden ? 0.5 : 1
                            Image {
                                id: picture
                                anchors.fill: parent
                                visible: status === Image.Ready
                                source: gameRow.modelData.icon
                                sourceSize.width: 64
                                sourceSize.height: 64
                                fillMode: Image.PreserveAspectCrop
                                asynchronous: true
                            }
                        }
                        Column {
                            anchors.left: tile.right
                            anchors.leftMargin: 12
                            anchors.right: keys.left
                            anchors.rightMargin: 12
                            anchors.verticalCenter: parent.verticalCenter
                            Text {
                                width: parent.width
                                textFormat: Text.PlainText
                                text: gameRow.modelData.name
                                elide: Text.ElideRight
                                color: gameRow.modelData.hidden ? Theme.ink3 : Theme.ink1
                                font: Theme.type.row
                            }
                            Text {
                                width: parent.width
                                textFormat: Text.PlainText
                                text: gameRow.modelData.hidden
                                    ? qsTr("Hidden · %1").arg(root.lastPlayed(gameRow.modelData))
                                    : root.lastPlayed(gameRow.modelData)
                                elide: Text.ElideRight
                                color: gameRow.modelData.playing && !gameRow.modelData.hidden ? Theme.accentInk : Theme.ink3
                                font: Theme.type.caption
                            }
                        }
                        Row {
                            id: keys
                            anchors.right: parent.right
                            anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 8
                            KitButton {
                                size: "sm"
                                variant: "ghost"
                                text: gameRow.modelData.hidden ? qsTr("Show") : qsTr("Hide")
                                Accessible.name: gameRow.modelData.hidden
                                    ? qsTr("Show %1").arg(gameRow.modelData.name)
                                    : qsTr("Hide %1").arg(gameRow.modelData.name)
                                onClicked: Games.setHidden(gameRow.modelData.key, !gameRow.modelData.hidden)
                            }
                            KitButton {
                                visible: gameRow.modelData.added
                                size: "sm"
                                variant: "ghost"
                                text: qsTr("Remove")
                                Accessible.name: qsTr("Remove %1").arg(gameRow.modelData.name)
                                onClicked: Games.remove(gameRow.modelData.key)
                            }
                        }
                    }
                }
            }
        }
    }

    SettingsCard {
        width: parent.width
        sectionId: "add-game"
        SettingsRow {
            rowId: "add-game"
            layout: "inline"
            KitButton {
                variant: "secondary"
                size: "sm"
                iconName: "add"
                text: root.picking ? qsTr("Refresh") : qsTr("Add it")
                loading: Games.loading
                onClicked: {
                    root.picking = true
                    Games.loadRunning()
                }
            }
        }
        SettingsRow {
            custom: true
            visible: root.picking
            hideLabel: true
            layout: "block"
            Column {
                width: parent.width
                spacing: 4
                Text {
                    visible: !Games.loading && root.running.length === 0
                    width: parent.width
                    textFormat: Text.PlainText
                    text: qsTr("Nothing with a window is running.")
                    color: Theme.ink3
                    font: Theme.type.description
                }
                Repeater {
                    model: root.running
                    delegate: Item {
                        id: programRow
                        required property var modelData
                        width: parent.width
                        height: 44
                        Column {
                            anchors.left: parent.left
                            anchors.leftMargin: 8
                            anchors.right: addKey.left
                            anchors.rightMargin: 12
                            anchors.verticalCenter: parent.verticalCenter
                            Text {
                                width: parent.width
                                textFormat: Text.PlainText
                                text: programRow.modelData.title.length > 0 ? programRow.modelData.title : programRow.modelData.exe
                                elide: Text.ElideRight
                                color: Theme.ink1
                                font: Theme.type.row
                            }
                            Text {
                                width: parent.width
                                textFormat: Text.PlainText
                                text: programRow.modelData.exe
                                elide: Text.ElideMiddle
                                color: Theme.ink3
                                font: Theme.type.caption
                            }
                        }
                        KitButton {
                            id: addKey
                            anchors.right: parent.right
                            anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            size: "sm"
                            variant: "ghost"
                            enabled: !programRow.modelData.added
                            text: programRow.modelData.added ? qsTr("Added") : qsTr("Add")
                            Accessible.name: qsTr("Add %1 as a game").arg(programRow.modelData.exe)
                            onClicked: Games.addProgram(programRow.modelData.exe, programRow.modelData.title)
                        }
                    }
                }
            }
        }
    }
}
