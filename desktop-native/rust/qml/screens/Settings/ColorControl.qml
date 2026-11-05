pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import com.yap.native

// A profile colour (SettingsAccountTab.tsx ColorControl): the 8 preset
// swatches, a "+" chip that opens the custom colour (a hex field), and
// "Auto" while it follows the app theme, or a ghost Reset back to Auto.
// `value` is `#RRGGBB` or empty for Auto.
Flow {
    id: root

    property string value
    // Lowercase noun for control names ("banner color").
    property string name
    readonly property list<string> presets: ["#FFFFFF", "#F4E6D4", "#FFD0BF", "#FCA5A5", "#F472B6", "#C084FC", "#60A5FA", "#34D399"]
    readonly property bool custom: value.length > 0 && !presets.includes(value)

    signal picked(string value)

    spacing: 12

    Row {
        spacing: 6
        Accessible.role: Accessible.Grouping
        Accessible.name: root.name
        Repeater {
            model: root.presets
            delegate: Swatch {
                required property string modelData
                fill: modelData
                on: root.value === modelData
                Accessible.name: modelData
                onClicked: root.picked(modelData)
            }
        }
        Swatch {
            id: plus
            fill: root.custom ? root.value : Theme.tint1
            on: root.custom
            Accessible.name: qsTr("Custom %1").arg(root.name)
            onClicked: popover.open()
            Icon {
                anchors.centerIn: parent
                visible: !root.custom
                name: "add"
                size: Theme.iconSm
                color: plus.hovered ? Theme.ink1 : Theme.ink2
            }

            T.Popup {
                id: popover
                y: plus.height + 8
                width: 240
                padding: 12
                focus: true
                closePolicy: T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutside
                onOpened: {
                    hex.text = root.value.replace(/^#/, "")
                    hex.forceActiveFocus()
                    hex.selectAll()
                }
                function commit() {
                    const text = hex.text.trim()
                    if (Settings.normalizeColor(text) === root.value || (text.length === 0 && root.value.length === 0))
                        return
                    if (text.length === 0) {
                        root.picked("")
                    } else {
                        const next = Settings.normalizeColor(text)
                        if (next.length > 0)
                            root.picked(next)
                    }
                }
                background: Rectangle {
                    radius: Theme.radius
                    color: Theme.ply3
                    border.width: 1
                    border.color: Theme.lineStrong
                }
                contentItem: Column {
                    spacing: 12
                    Text {
                        textFormat: Text.PlainText
                        text: qsTr("Custom %1").arg(root.name)
                        color: Theme.ink1
                        font: Theme.type.occupant
                    }
                    Row {
                        spacing: 8
                        Rectangle {
                            width: 32
                            height: 32
                            radius: Theme.radius
                            color: Settings.normalizeColor(hex.text).length > 0 ? Settings.normalizeColor(hex.text) : Theme.tint1
                            border.width: 1
                            border.color: Theme.lineStrong
                        }
                        KitField {
                            id: hex
                            small: true
                            width: 240 - 24 - 40
                            leadingInset: 10
                            maximumLength: 7
                            placeholderText: qsTr("Auto")
                            font: Theme.type.code
                            Accessible.name: qsTr("Hex %1").arg(root.name)
                            onAccepted: {
                                popover.commit()
                                popover.close()
                            }
                            onEditingFinished: popover.commit()
                            Text {
                                x: 10
                                anchors.verticalCenter: parent.verticalCenter
                                text: "#"
                                color: Theme.ink3
                                font: Theme.type.code
                            }
                        }
                    }
                }
            }
        }
    }

    Text {
        visible: root.value.length === 0
        height: 28
        verticalAlignment: Text.AlignVCenter
        text: qsTr("Auto")
        color: Theme.ink3
        font: Theme.type.caption
    }
    KitButton {
        visible: root.value.length > 0
        variant: "ghost"
        size: "sm"
        text: qsTr("Reset")
        Accessible.name: qsTr("Use the default %1").arg(root.name)
        onClicked: root.picked("")
    }

    // A 28 px swatch; chosen is a 2 px accent ring 2 px out.
    component Swatch: T.AbstractButton {
        id: swatch
        property color fill
        property bool on: false
        width: 28
        height: 28
        focusPolicy: Qt.StrongFocus
        scale: down ? Theme.scalePress : 1
        Accessible.role: Accessible.Button
        Accessible.checkable: true
        Accessible.checked: on
        background: Item {
            Rectangle {
                anchors.fill: parent
                radius: 4
                color: swatch.fill
                border.width: 1
                border.color: Theme.lineStrong
            }
            Rectangle {
                anchors.fill: parent
                anchors.margins: -4
                visible: swatch.on || swatch.visualFocus
                radius: 6
                color: "transparent"
                border.width: 2
                border.color: Theme.accentInk
            }
        }
    }
}
