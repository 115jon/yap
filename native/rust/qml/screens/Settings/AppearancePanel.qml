pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// Appearance (AppearancePanel.tsx): the theme as a grid of thumbnails (each
// theme's own colours, Match system split light and dark), synced to the
// account when "Use this theme on all my devices" is on; the preview
// sheet; and reduced motion, which follows the device.
Column {
    id: root

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
                    model: Theme.themeCatalog
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
                                    ThemeMock { t: Theme.themeMock(tile.modelData.id) }
                                }
                                Component {
                                    id: splitMock
                                    Item {
                                        Item {
                                            width: parent.width / 2
                                            height: parent.height
                                            clip: true
                                            ThemeMock { width: parent.width * 2; height: parent.height; t: Theme.themeMock("dark") }
                                        }
                                        Item {
                                            x: parent.width / 2
                                            width: parent.width / 2
                                            height: parent.height
                                            clip: true
                                            ThemeMock { x: -parent.width; width: parent.width * 2; height: parent.height; t: Theme.themeMock("light") }
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
