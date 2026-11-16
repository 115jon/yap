pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Settings > Appearance: the core's themes (kept on this device in
// appearance.json and synced to the account when asked), then Text size
// (a factor over the system's, this phone's own, Prefs.textFactor).
SettingsFrame {
    id: root

    // Text size factors over the system's scale.
    readonly property var textSizes: [
        { factor: 0.85, label: qsTr("Small") },
        { factor: 1, label: qsTr("Default") },
        { factor: 1.15, label: qsTr("Large") },
        { factor: 1.3, label: qsTr("Larger") }
    ]

    title: qsTr("Appearance")

    SettingsSection {
        number: 1
        title: qsTr("Theme")
        ThemeTile {
            width: parent.width
            themeId: "system"
            label: qsTr("Match system")
        }
        Item { width: 1; height: 12 }
        Grid {
            width: parent.width
            columns: 2
            columnSpacing: 12
            rowSpacing: 12
            Repeater {
                model: Theme.themePalettes
                delegate: ThemeTile {
                    required property var modelData
                    width: (parent.width - 12) / 2
                    themeId: modelData.id
                    label: modelData.label
                }
            }
        }
        Item { width: 1; height: 8 }
        SettingsSwitchRow {
            text: qsTr("Use on all my devices")
            description: qsTr("Your theme follows your account.")
            checked: Settings.themeSync
            onToggled: Settings.changeThemeSync(!Settings.themeSync)
        }
    }

    SettingsSection {
        number: 2
        title: qsTr("Text size")
        Segmented {
            width: parent.width
            accessibleName: qsTr("Text size")
            options: root.textSizes.map(size => size.label)
            current: {
                const at = root.textSizes.findIndex(size => Math.abs(size.factor - Prefs.textFactor) < 0.01)
                return at < 0 ? 1 : at
            }
            onPicked: index => Prefs.textFactor = root.textSizes[index].factor
        }
        Item { width: 1; height: 12 }
        // A message at the chosen size (the type roles already carry
        // Theme.textScale).
        Rectangle {
            width: parent.width
            height: sample.height + 24
            radius: Theme.radius
            color: Theme.ply1
            border.width: 1
            border.color: Theme.line
            Row {
                id: sample
                x: 12
                y: 12
                width: parent.width - 24
                spacing: 12
                Avatar {
                    id: sampleFace
                    size: 32
                    initials: Session.meInitials.length > 0 ? Session.meInitials : "?"
                    source: Session.meAvatar
                }
                Column {
                    width: parent.width - sampleFace.width - parent.spacing
                    spacing: 2
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: Session.meName
                        elide: Text.ElideRight
                        color: Theme.ink1
                        font: Theme.type.author
                    }
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: qsTr("Messages, names and menus read at this size.")
                        wrapMode: Text.Wrap
                        color: Theme.ink1
                        font: Theme.type.message
                    }
                }
            }
        }
        Item { width: 1; height: 8 }
        Text {
            width: parent.width
            wrapMode: Text.Wrap
            text: qsTr("On top of your phone's text size (%1%).").arg(Math.round(Native.fontScale * 100))
            color: Theme.ink3
            font: Theme.type.caption
        }
    }

    // A theme's thumbnail (its own colours; Match system split dark and
    // light) over its name, ringed in ink-1 with a check while chosen.
    component ThemeTile: Item {
        id: tile
        property string themeId
        property string label
        readonly property bool on: Settings.theme === themeId
        height: thumb.height + 8 + 18
        Accessible.role: Accessible.RadioButton
        Accessible.name: label
        Accessible.checkable: true
        Accessible.checked: on
        Accessible.onPressAction: Settings.chooseTheme(tile.themeId)
        scale: tileTap.pressed ? Theme.scalePress : 1
        Behavior on scale { NumberAnimation { duration: Theme.dur1 } }
        Rectangle {
            id: thumb
            width: parent.width
            height: tile.themeId === "system" ? 72 : Math.round(width * 9 / 16)
            radius: Theme.radius
            color: "transparent"
            clip: true
            Item {
                anchors.fill: parent
                anchors.margins: 2
                clip: true
                Loader {
                    width: parent.width
                    height: parent.height
                    active: tile.themeId !== "system"
                    sourceComponent: Mock { t: Theme.themeMock(tile.themeId) }
                }
                Item {
                    visible: tile.themeId === "system"
                    width: parent.width / 2
                    height: parent.height
                    clip: true
                    Mock { width: parent.width * 2; height: parent.height; t: Theme.themeMock("dark") }
                }
                Item {
                    visible: tile.themeId === "system"
                    x: parent.width / 2
                    width: parent.width / 2
                    height: parent.height
                    clip: true
                    Mock { x: -parent.width; width: parent.width * 2; height: parent.height; t: Theme.themeMock("light") }
                }
            }
            Rectangle {
                anchors.fill: parent
                color: "transparent"
                radius: Theme.radius
                border.width: tile.on ? 2 : 1
                border.color: tile.on ? Theme.ink1 : Theme.lineStrong
            }
            Rectangle {
                visible: tile.on
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 6
                width: 20
                height: 20
                radius: Theme.radius
                color: Theme.primaryFill
                Icon {
                    anchors.centerIn: parent
                    name: "check"
                    size: Theme.iconXs
                    color: Theme.primaryFillInk
                }
            }
        }
        Text {
            y: thumb.height + 8
            width: parent.width
            textFormat: Text.PlainText
            text: tile.label
            elide: Text.ElideRight
            color: tile.on ? Theme.ink1 : Theme.ink2
            font: tile.on ? Theme.type.labelStrong : Theme.type.description
        }
        TapHandler {
            id: tileTap
            onTapped: {
                if (tile.on)
                    return
                Native.haptic(Native.Tick)
                Settings.chooseTheme(tile.themeId)
            }
        }
    }

    // A theme drawn small: the rail, the sidebar, a header line, two
    // messages and the accent.
    component Mock: Rectangle {
        id: mock
        required property var t
        color: t.bg
        Rectangle { width: parent.width * 0.1; height: parent.height; color: mock.t.rail }
        Rectangle { x: parent.width * 0.1; width: parent.width * 0.26; height: parent.height; color: mock.t.sidebar }
        Rectangle { x: parent.width * 0.14; y: parent.height * 0.18; width: parent.width * 0.16; height: 3; color: mock.t.dim }
        Rectangle { x: parent.width * 0.14; y: parent.height * 0.32; width: parent.width * 0.12; height: 3; color: mock.t.accent }
        Rectangle { x: parent.width * 0.14; y: parent.height * 0.46; width: parent.width * 0.18; height: 3; color: mock.t.dim }
        Rectangle { x: parent.width * 0.42; y: parent.height * 0.24; width: parent.width * 0.32; height: 4; color: mock.t.text; opacity: 0.85 }
        Rectangle { x: parent.width * 0.42; y: parent.height * 0.38; width: parent.width * 0.48; height: 3; color: mock.t.dim }
        Rectangle { x: parent.width * 0.42; y: parent.height * 0.58; width: parent.width * 0.26; height: 4; color: mock.t.text; opacity: 0.85 }
        Rectangle { x: parent.width * 0.42; y: parent.height * 0.72; width: parent.width * 0.4; height: 3; color: mock.t.dim }
        Rectangle { x: parent.width * 0.06 - 2; y: parent.height * 0.14; width: 4; height: 4; color: mock.t.accent }
    }
}
