pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import com.yap.native

// Appearance (AppearancePanel.tsx): the theme as a grid of thumbnails (each
// theme's own colours, Match system split light and dark), synced to the
// account when "Use this theme on all my devices" is on; the preview
// sheet; and reduced motion, which follows the device.
Column {
    id: root

    // Hand-matched to each theme's tokens, for the thumbnails (SWATCHES).
    readonly property var swatches: ({
        "dark": { bg: "#0b0b0d", rail: "#000000", sidebar: "#141417", elevated: "#232328", accent: "#ff8ad8", text: "#f2f2f3", textDim: Qt.rgba(0.949, 0.949, 0.953, 0.35), border: Qt.rgba(0.949, 0.949, 0.953, 0.08) },
        "light": { bg: "#fafafa", rail: "#f4f4f5", sidebar: "#f0f0f2", elevated: "#ffffff", accent: "#c0127a", text: "#0b0b0d", textDim: Qt.rgba(0.043, 0.043, 0.051, 0.3), border: Qt.rgba(0.043, 0.043, 0.051, 0.09) },
        "miku-dark": { bg: "#0c0b14", rail: "#07060d", sidebar: "#0f0e1a", elevated: "#1b192e", accent: "#39c5bb", text: "#ffffff", textDim: Qt.rgba(0.698, 0.722, 0.816, 0.45), border: "#23213a" },
        "miku-light": { bg: "#f3faf9", rail: "#d3ede8", sidebar: "#e1f3f0", elevated: "#ffffff", accent: "#00a399", text: "#1b2240", textDim: Qt.rgba(0.239, 0.278, 0.4, 0.35), border: "#c0e7e1" },
        "spiderman-dark": { bg: "#020208", rail: "#000000", sidebar: "#03030b", elevated: "#0d0d22", accent: "#e50914", text: "#ffffff", textDim: Qt.rgba(0.804, 0.824, 0.878, 0.4), border: Qt.rgba(0.102, 0.451, 0.91, 0.35) },
        "spiderman-light": { bg: "#f8f9fc", rail: "#e8ebf3", sidebar: "#f1f3f9", elevated: "#ffffff", accent: "#e50914", text: "#0f172a", textDim: Qt.rgba(0.278, 0.333, 0.412, 0.35), border: Qt.rgba(0.059, 0.09, 0.165, 0.08) }
    })

    spacing: 40

    SettingsCard {
        width: parent.width
        sectionId: "theme"

        SettingsRow {
            rowId: "theme"
            layout: "block"
            hideLabel: true
            Grid {
                id: grid
                // The web's @container is the settings pane (SettingsWorkbench
                // main column: the dialog less its 16 px insets and 264 rail),
                // not this card, so desktop sizes always get four.
                readonly property real pane: (Window.width > 0 ? Window.width : 1280) - 32 - 264
                readonly property int cols: pane >= 640 ? 4 : pane >= 480 ? 3 : 2
                width: parent.width
                columns: cols
                columnSpacing: 12
                rowSpacing: 16
                Accessible.role: Accessible.Grouping
                Accessible.name: SettingsNav.label("theme")
                Repeater {
                    model: SettingsNav.themes
                    delegate: T.AbstractButton {
                        id: tile
                        required property var modelData
                        readonly property bool on: Settings.theme === modelData.id
                        width: (grid.width - grid.columnSpacing * (grid.cols - 1)) / grid.cols
                        height: thumb.height + 6 + 20
                        focusPolicy: Qt.StrongFocus
                        scale: down && !Theme.reducedMotion ? 0.97 : 1
                        Behavior on scale { NumberAnimation { duration: 150 } }
                        Accessible.role: Accessible.RadioButton
                        Accessible.name: modelData.label
                        Accessible.checkable: true
                        Accessible.checked: on
                        onClicked: Settings.chooseTheme(modelData.id)

                        Rectangle {
                            id: thumb
                            width: parent.width
                            height: Math.round(width * 10 / 16)
                            radius: Theme.radius
                            color: "transparent"
                            clip: true
                            Loader {
                                anchors.fill: parent
                                anchors.margins: 1
                                sourceComponent: tile.modelData.id === "system" ? splitMock : singleMock
                                Component {
                                    id: singleMock
                                    ThemeMock { t: root.swatches[tile.modelData.id] }
                                }
                                Component {
                                    id: splitMock
                                    Item {
                                        Item {
                                            width: parent.width / 2
                                            height: parent.height
                                            clip: true
                                            ThemeMock { width: parent.width * 2; height: parent.height; t: root.swatches["dark"] }
                                        }
                                        Item {
                                            x: parent.width / 2
                                            width: parent.width / 2
                                            height: parent.height
                                            clip: true
                                            ThemeMock { x: -parent.width; width: parent.width * 2; height: parent.height; t: root.swatches["light"] }
                                        }
                                    }
                                }
                            }
                            Rectangle {
                                anchors.fill: parent
                                color: "transparent"
                                radius: Theme.radius
                                border.width: tile.on ? 0 : 1
                                border.color: Theme.lineStrong
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
                        // The chosen ring: 2 px ink-1, 2 px off the thumbnail.
                        Rectangle {
                            visible: tile.on || tile.visualFocus
                            x: -4
                            y: -4
                            width: thumb.width + 8
                            height: thumb.height + 8
                            radius: Theme.radius
                            color: "transparent"
                            border.width: 2
                            border.color: tile.visualFocus ? Theme.focus : Theme.ink1
                        }
                        Text {
                            y: thumb.height + 6
                            width: parent.width
                            leftPadding: 2
                            textFormat: Text.PlainText
                            text: tile.modelData.label
                            elide: Text.ElideRight
                            color: tile.on ? Theme.ink1 : Theme.ink2
                            font: tile.on ? Theme.type.rowUnread : Theme.type.description
                        }
                        HoverHandler { cursorShape: Qt.PointingHandCursor }
                    }
                }
            }
        }

        SwitchSetting {
            rowId: "theme-sync"
            checked: Settings.themeSync
            onToggled: Settings.changeThemeSync(!Settings.themeSync)
        }

        SettingsRow {
            rowId: "theme-preview"
            layout: "inline"
            KitButton {
                iconName: "eye"
                text: qsTr("Preview")
                onClicked: SettingsNav.previewing = true
            }
        }
    }

    SettingsCard {
        width: parent.width
        sectionId: "motion"
        SettingsRow {
            rowId: "reduced-motion"
            layout: "inline"
            Text {
                text: qsTr("Follows your device")
                color: Theme.ink2
                font: Theme.type.row
            }
        }
    }

    // A theme thumbnail (ThemeMock): rail, sidebar and a chat column in the
    // theme's own colours.
    component ThemeMock: Rectangle {
        id: mock
        required property var t
        color: t.bg
        Rectangle {
            id: mockRail
            width: parent.width * 0.14
            height: parent.height
            color: mock.t.rail
            Column {
                y: parent.height * 0.08
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: mock.height * 0.06
                Rectangle { width: mockRail.width * 0.6; height: width; radius: width / 2; color: mock.t.accent }
                Rectangle { width: mockRail.width * 0.6; height: width; radius: width / 2; color: mock.t.elevated }
            }
        }
        Rectangle {
            id: mockSide
            x: mockRail.width
            width: parent.width * 0.28
            height: parent.height
            color: mock.t.sidebar
            Column {
                x: parent.width * 0.05 * 4
                y: parent.height * 0.1
                width: parent.width * 0.8
                spacing: mock.height * 0.07
                Rectangle { width: parent.width * 0.8; height: mock.height * 0.06; radius: height / 2; color: mock.t.textDim }
                Rectangle { width: parent.width * 0.6; height: mock.height * 0.06; radius: height / 2; color: mock.t.textDim }
                Rectangle { width: parent.width; height: mock.height * 0.12; radius: 3; color: mock.t.elevated; border.width: 1; border.color: mock.t.border }
            }
        }
        Column {
            x: mockSide.x + mockSide.width + parent.width * 0.035
            width: parent.width - x - parent.width * 0.035
            anchors.bottom: parent.bottom
            anchors.bottomMargin: parent.height * 0.06
            spacing: mock.height * 0.08
            Row {
                spacing: parent.width * 0.06
                Rectangle { width: mock.width * 0.08; height: width; radius: width / 2; color: mock.t.elevated }
                Rectangle { anchors.verticalCenter: parent.verticalCenter; width: mock.width * 0.33; height: Math.max(3, mock.height * 0.1); radius: height / 2; color: mock.t.textDim }
            }
            Row {
                spacing: parent.width * 0.06
                Rectangle { width: mock.width * 0.08; height: width; radius: width / 2; color: mock.t.accent }
                Rectangle { anchors.verticalCenter: parent.verticalCenter; width: mock.width * 0.25; height: Math.max(3, mock.height * 0.1); radius: height / 2; color: mock.t.text; opacity: 0.85 }
            }
            Rectangle { width: parent.width; height: Math.max(6, mock.height * 0.18); radius: 3; color: mock.t.elevated; border.width: 1; border.color: mock.t.border }
        }
    }
}
