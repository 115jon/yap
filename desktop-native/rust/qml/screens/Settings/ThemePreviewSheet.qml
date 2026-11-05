pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import com.yap.native

// The live theme preview (ThemePreviewSidebar.tsx): a right side sheet
// with no scrim, so the app behind shows each theme as you pick it. Escape,
// the close key and "Back to settings" return to the same page.
T.Drawer {
    id: sheet

    readonly property var options: [
        { id: "light", label: qsTr("Light"), stops: ["#ffffff", "#f4f4f5", "#c0127a"] },
        { id: "dark", label: qsTr("Midnight"), stops: ["#141417", "#0b0b0d", "#ff8ad8"] },
        { id: "system", label: qsTr("System"), stops: ["#0b0b0d", "#80808a", "#f4f4f5"] },
        { id: "miku-light", label: qsTr("Miku Light"), stops: ["#ffffff", "#e8f4fd", "#f872a5"] },
        { id: "miku-dark", label: qsTr("Miku Dark"), stops: ["#13111f", "#0f0d19", "#f872a5"] },
        { id: "spiderman-light", label: qsTr("Spider-Man Light"), stops: ["#ffffff", "#eef0f6", "#E50914"] },
        { id: "spiderman-dark", label: qsTr("Spider-Man Dark"), stops: ["#06050a", "#0b0a10", "#E50914"] },
    ]

    edge: Qt.RightEdge
    width: 360
    height: parent ? parent.height : 0
    modal: false
    focus: true
    interactive: false
    closePolicy: T.Popup.CloseOnEscape
    readonly property bool wanted: SettingsNav.open && SettingsNav.previewing
    onWantedChanged: wanted ? open() : close()
    onClosed: SettingsNav.previewing = false

    enter: Transition {
        NumberAnimation { property: "position"; to: 1; duration: Theme.dur4; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeDrawer }
    }
    exit: Transition {
        NumberAnimation { property: "position"; to: 0; duration: Theme.dur4Exit; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeDrawer }
    }
    background: Rectangle {
        color: Theme.ply3
        Rectangle {
            width: 1
            height: parent.height
            color: Theme.lineStrong
        }
    }

    contentItem: Item {
        Item {
            id: head
            width: parent.width
            height: Theme.header
            Text {
                x: 16
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Preview theme")
                color: Theme.ink1
                font: Theme.type.panelTitle
                Accessible.role: Accessible.Heading
            }
            IconKey {
                anchors.right: parent.right
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                size: 32
                iconName: "close"
                tone: "muted"
                tip: qsTr("Close preview")
                tipSide: "bottom"
                onClicked: SettingsNav.previewing = false
            }
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line }
        }
        Flickable {
            anchors.top: head.bottom
            anchors.bottom: foot.top
            width: parent.width
            clip: true
            contentHeight: grid.y + grid.height + 16
            boundsBehavior: Flickable.StopAtBounds
            Text {
                id: lede
                x: 16
                y: 16
                width: parent.width - 32
                text: qsTr("Switch themes and see the current screen update live.")
                wrapMode: Text.Wrap
                color: Theme.ink2
                font: Theme.type.description
            }
            Grid {
                id: grid
                x: 16
                y: lede.y + lede.height + 16
                width: parent.width - 32
                columns: 2
                spacing: 12
                Repeater {
                    model: sheet.options
                    delegate: T.AbstractButton {
                        id: option
                        required property var modelData
                        readonly property bool on: Settings.theme === modelData.id
                        width: (grid.width - 12) / 2
                        height: 80 + 8 + 20 + 16
                        focusPolicy: Qt.StrongFocus
                        scale: down ? Theme.scalePress : 1
                        Accessible.role: Accessible.Button
                        Accessible.name: modelData.label
                        Accessible.checkable: true
                        Accessible.checked: on
                        onClicked: Settings.chooseTheme(modelData.id)
                        background: Rectangle {
                            radius: Theme.radiusSheet
                            color: option.on ? Theme.accentTint : Theme.tint1
                            border.width: option.visualFocus ? 2 : 1
                            border.color: option.visualFocus ? Theme.focus : option.on ? Theme.accentInk : Theme.line
                        }
                        contentItem: Column {
                            padding: 8
                            spacing: 8
                            Rectangle {
                                width: option.width - 16
                                height: 80
                                radius: Theme.radius
                                gradient: Gradient {
                                    orientation: Gradient.Horizontal
                                    GradientStop { position: 0; color: option.modelData.stops[0] }
                                    GradientStop { position: 0.5; color: option.modelData.stops[1] }
                                    GradientStop { position: 1; color: option.modelData.stops[2] }
                                }
                            }
                            Text {
                                width: option.width - 16
                                textFormat: Text.PlainText
                                text: option.modelData.label
                                elide: Text.ElideRight
                                color: Theme.ink1
                                font: Theme.type.occupant
                            }
                        }
                    }
                }
            }
        }
        Item {
            id: foot
            anchors.bottom: parent.bottom
            width: parent.width
            height: 72
            Rectangle { width: parent.width; height: 1; color: Theme.line }
            KitButton {
                x: 16
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - 32
                iconName: "chevronLeft"
                text: qsTr("Back to settings")
                onClicked: SettingsNav.previewing = false
            }
        }
    }
}
