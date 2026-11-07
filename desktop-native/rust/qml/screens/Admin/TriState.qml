pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import com.yap.native

// Deny / Inherit / Allow for one permission (PermissionTriState.tsx): three
// keys in a 1 px line-strong frame, the chosen one filled (danger tint,
// tint-2, or the success ink at 14 %), 600 and its glyph, never colour
// alone. Arrow keys move the choice; the group is named by its row.
Rectangle {
    id: root

    // `deny`, `inherit` or `allow`.
    property string value: "inherit"
    property string accessibleName
    property bool disabled: false

    signal picked(string value)

    readonly property var options: [
        { value: "deny", label: qsTr("Deny"), icon: "close" },
        { value: "inherit", label: qsTr("Inherit"), icon: "minus" },
        { value: "allow", label: qsTr("Allow"), icon: "check" },
    ]

    function step(by) {
        const at = options.findIndex(option => option.value === value)
        const next = options[(at + by + options.length) % options.length]
        picked(next.value)
    }

    implicitWidth: 3 * 92 + 2 * 2 + 4
    implicitHeight: 32 + 4
    radius: Theme.radius
    color: "transparent"
    border.width: 1
    border.color: Theme.lineStrong
    opacity: disabled ? 0.45 : 1
    activeFocusOnTab: !disabled
    Accessible.role: Accessible.RadioButton
    Accessible.name: accessibleName
    Accessible.description: options.find(option => option.value === value)?.label ?? ""
    Keys.onLeftPressed: if (!disabled) step(-1)
    Keys.onRightPressed: if (!disabled) step(1)

    Rectangle {
        anchors.fill: parent
        anchors.margins: -3
        visible: root.activeFocus
        color: "transparent"
        radius: Theme.radius
        border.width: 2
        border.color: Theme.focus
    }

    Row {
        x: 2
        y: 2
        spacing: 2
        Repeater {
            model: root.options
            delegate: T.AbstractButton {
                id: key
                required property var modelData
                readonly property bool chosen: root.value === modelData.value
                width: (root.width - 4 - 4) / 3
                height: 32
                enabled: !root.disabled
                focusPolicy: Qt.NoFocus
                Accessible.role: Accessible.RadioButton
                Accessible.name: modelData.label
                Accessible.checked: chosen
                onClicked: root.picked(modelData.value)
                background: Rectangle {
                    radius: Theme.radius
                    color: !key.chosen ? (key.hovered ? Theme.tint1 : "transparent")
                        : key.modelData.value === "deny" ? Theme.dangerTint
                        : key.modelData.value === "allow" ? Qt.rgba(Theme.successInk.r, Theme.successInk.g, Theme.successInk.b, 0.14)
                        : Theme.tint2
                    Behavior on color { ColorAnimation { duration: Theme.dur1 } }
                }
                contentItem: Item {
                    Row {
                        anchors.centerIn: parent
                        spacing: 6
                        readonly property color ink: !key.chosen ? (key.hovered ? Theme.ink1 : Theme.ink2)
                            : key.modelData.value === "deny" ? Theme.dangerInk
                            : key.modelData.value === "allow" ? Theme.successInk : Theme.ink1
                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: key.modelData.icon
                            variant: key.chosen ? "active" : "idle"
                            size: Theme.iconSm
                            color: parent.ink
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: key.modelData.label
                            color: parent.ink
                            font: key.chosen ? Theme.type.rowUnread : Theme.type.row
                        }
                    }
                }
                HoverHandler { cursorShape: Qt.PointingHandCursor }
            }
        }
    }
}
