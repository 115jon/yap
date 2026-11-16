import QtQuick
import Yap.Mobile
import Yap.Ui

// A choice of two to four (DESIGN.md Segmented rail): a 2 px ink-1 rail,
// cells split by 2 px rules, the chosen cell lit in primary-fill.
Rectangle {
    id: rail
    property var options: []
    property int current: 0
    property string accessibleName
    signal picked(int index)
    height: 48
    radius: Theme.radius
    color: "transparent"
    border.width: 2
    border.color: Theme.ink1
    Accessible.role: Accessible.Grouping
    Accessible.name: accessibleName
    Row {
        x: 2
        y: 2
        width: parent.width - 4
        height: parent.height - 4
        Repeater {
            model: rail.options
            delegate: Item {
                id: cell
                required property int index
                required property string modelData
                readonly property bool on: rail.current === index
                width: parent.width / rail.options.length
                height: parent.height
                Accessible.role: Accessible.RadioButton
                Accessible.name: modelData
                Accessible.checkable: true
                Accessible.checked: on
                Accessible.onPressAction: rail.picked(index)
                Rectangle {
                    anchors.fill: parent
                    color: cell.on ? Theme.primaryFill : cellTap.pressed ? Theme.tint2 : "transparent"
                    Behavior on color { ColorAnimation { duration: Theme.dur1 } }
                }
                Rectangle {
                    visible: cell.index > 0
                    width: 2
                    height: parent.height
                    color: Theme.ink1
                }
                Text {
                    anchors.centerIn: parent
                    width: parent.width - 8
                    horizontalAlignment: Text.AlignHCenter
                    textFormat: Text.PlainText
                    text: cell.modelData
                    // A long label ("Friends & servers") shrinks a
                    // little to fit a narrow phone before it elides.
                    fontSizeMode: Text.HorizontalFit
                    minimumPixelSize: 11
                    minimumPointSize: 8
                    elide: Text.ElideRight
                    color: cell.on ? Theme.primaryFillInk : Theme.ink1
                    font: Theme.type.controlSm
                }
                TapHandler {
                    id: cellTap
                    onTapped: {
                        if (cell.on)
                            return
                        Native.haptic(Native.Tick)
                        rail.picked(cell.index)
                    }
                }
            }
        }
    }
}
