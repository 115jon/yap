pragma ComponentBehavior: Bound
import QtQuick
import Yap.Ui

// Your presence as a segmented rail (the account panel's): Online, Idle,
// DND and Invisible side by side inside a 2 px ink edge, each an LED over
// its silkscreen word; the chosen cell is the primary fill. `chose` gives
// the presence value (Session.setStatus's: online, idle, dnd, offline).
Rectangle {
    id: root

    // The presence shown as chosen.
    property string status: "online"
    signal chose(string value)

    readonly property var options: [
        { value: "online", label: qsTr("Online"), name: qsTr("Online") },
        { value: "idle", label: qsTr("Idle"), name: qsTr("Idle") },
        { value: "dnd", label: qsTr("DND"), name: qsTr("Do not disturb") },
        { value: "offline", label: qsTr("Invisible"), name: qsTr("Invisible") }
    ]

    // Every cell is as tall as an LED over a silkscreen line, a thumb at
    // least (it grows with the text size).
    FontMetrics { id: silk; font: Theme.type.silk }
    readonly property real cellHeight: Math.max(44, Theme.hitMin, Theme.led + 4 + Math.ceil(silk.height) + 12)

    // One word size for all four cells: the silkscreen, shrunk only as far
    // as the widest word needs to fit its cell (a large text size).
    // Measured as drawn (capitals and letter spacing), never shown.
    Text {
        id: widest
        visible: false
        font: Theme.type.silk
        text: qsTr("Invisible")
    }
    readonly property real fit: Math.min(1, (width / options.length - 16) / Math.max(1, widest.implicitWidth))
    readonly property font cellFont: {
        const f = Theme.type.silk
        return Qt.font({ family: f.family, pixelSize: Math.max(8, Math.floor(f.pixelSize * fit)), weight: f.weight,
                         letterSpacing: f.letterSpacing * fit, capitalization: f.capitalization,
                         hintingPreference: f.hintingPreference })
    }

    implicitWidth: 280
    implicitHeight: cellHeight + 4
    radius: Theme.radius
    color: "transparent"
    border.width: 2
    border.color: Theme.ink1
    Accessible.role: Accessible.Grouping
    Accessible.name: qsTr("Status")

    Row {
        id: cells
        x: 2
        y: 2
        width: parent.width - 4
        Repeater {
            model: root.options
            delegate: Rectangle {
                id: cell
                required property var modelData
                required property int index
                readonly property bool chosen: root.status === modelData.value
                width: cells.width / root.options.length
                height: root.cellHeight
                color: chosen ? Theme.primaryFill : tap.pressed ? Theme.tint2 : "transparent"
                EaseColor on color {}
                Accessible.role: Accessible.RadioButton
                Accessible.name: modelData.name
                Accessible.checked: chosen
                Accessible.onPressAction: root.chose(cell.modelData.value)

                // The 2 px ink divider before every cell but the first.
                Rectangle {
                    visible: cell.index > 0
                    width: 2
                    height: parent.height
                    color: Theme.ink1
                }
                Column {
                    id: stack
                    anchors.centerIn: parent
                    width: parent.width - 8
                    spacing: 4
                    Led {
                        anchors.horizontalCenter: parent.horizontalCenter
                        lit: cell.modelData.value !== "offline"
                        color: Theme.presenceLed(cell.modelData.value)
                    }
                    Text {
                        width: parent.width
                        horizontalAlignment: Text.AlignHCenter
                        textFormat: Text.PlainText
                        text: cell.modelData.label
                        elide: Text.ElideRight
                        color: cell.chosen ? Theme.primaryFillInk : Theme.ink2
                        font: root.cellFont
                    }
                }
                TapHandler {
                    id: tap
                    onTapped: root.chose(cell.modelData.value)
                }
            }
        }
    }
}
