pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// A profile colour (SettingsAccountTab.tsx ColorControl): the 8 preset
// swatches, a "+" chip that opens the colour picker (ColorPopover: the
// square, hue, hex, eyedropper and recents; the colour applies as it
// changes, Esc puts the old one back), and
// "Auto" while it follows the app theme, or a ghost Reset back to Auto.
// `value` is `#RRGGBB` or empty for Auto. Without `allowAuto` (the name
// style's colours) there is no Auto: Reset shows while the value is not
// `fallback` and goes back to it.
Flow {
    id: root

    property string value
    // Lowercase noun for control names ("banner color").
    property string name
    // PROFILE_COLOR_SWATCHES (DISPLAY_NAME_COLOR_SWATCHES for a name).
    property list<string> presets: ["#FFFFFF", "#F4E6D4", "#FFD0BF", "#FCA5A5", "#F472B6", "#C084FC", "#60A5FA", "#34D399"]
    property bool allowAuto: true
    property string fallback: ""
    readonly property bool custom: value.length > 0 && !presets.includes(value)
    readonly property bool isDefault: allowAuto ? value.length === 0 : value === fallback

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
            onClicked: popover.request(root.value)
            Icon {
                anchors.centerIn: parent
                visible: !root.custom
                name: "add"
                size: Theme.iconSm
                color: plus.hovered ? Theme.ink1 : Theme.ink2
            }

            ColorPopover {
                id: popover
                name: root.name
                start: root.value.length > 0 ? root.value : root.fallback
                onPicked: value => {
                    if (value !== root.value)
                        root.picked(value)
                }
            }
        }
    }

    Text {
        visible: root.allowAuto && root.isDefault
        height: 28
        verticalAlignment: Text.AlignVCenter
        text: qsTr("Auto")
        color: Theme.ink3
        font: Theme.type.caption
    }
    KitButton {
        visible: !root.isDefault
        variant: "ghost"
        size: "sm"
        text: qsTr("Reset")
        Accessible.name: qsTr("Use the default %1").arg(root.name)
        onClicked: root.picked(root.allowAuto ? "" : root.fallback)
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
        HoverHandler { cursorShape: Qt.PointingHandCursor }
    }
}
