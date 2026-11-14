pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// Two to four choices as the C1 segmented rail (primitives.tsx
// SegmentedField, ui/segmented-control.tsx): a 2 px ink-1 frame on ply-0,
// 44 px cells split by 2 px rules, the chosen cell primary-fill. Under it,
// the chosen option's tag and description, and "Saving…" while `busy`.
// `options`: [{value, label, desc, badge}].
Column {
    id: root

    property var options: []
    property string value
    property bool busy: false
    property string accessibleName

    signal picked(string value)

    readonly property var chosen: options.find(option => option.value === value) ?? null

    spacing: 12

    Rectangle {
        id: rail
        width: parent.width
        height: 44 + 4
        radius: Theme.radius
        color: Theme.ply0
        border.width: 2
        border.color: root.enabled ? Theme.ink1 : Theme.lineStrong
        opacity: root.enabled ? 1 : 0.6
        Accessible.role: Accessible.Grouping
        Accessible.name: root.accessibleName

        Row {
            x: 2
            y: 2
            width: parent.width - 4
            height: 44
            Repeater {
                model: root.options
                delegate: T.AbstractButton {
                    id: cell
                    required property var modelData
                    required property int index
                    readonly property bool on: modelData.value === root.value
                    width: parent.width / root.options.length
                    height: 44
                    focusPolicy: Qt.StrongFocus
                    Accessible.role: Accessible.RadioButton
                    Accessible.name: modelData.label
                    Accessible.checkable: true
                    Accessible.checked: on
                    onClicked: root.picked(modelData.value)
                    Keys.onLeftPressed: root.step(-1)
                    Keys.onRightPressed: root.step(1)

                    background: Rectangle {
                        color: cell.on ? Theme.primaryFill : "transparent"
                        Rectangle {
                            anchors.fill: parent
                            color: cell.on ? Theme.primaryFillInk : Theme.ink2
                            opacity: !cell.enabled ? 0 : cell.down ? 0.14 : cell.hovered ? 0.08 : 0
                        }
                        Rectangle {
                            visible: cell.index > 0
                            width: 2
                            height: parent.height
                            color: root.enabled ? Theme.ink1 : Theme.lineStrong
                        }
                        Rectangle {
                            anchors.fill: parent
                            anchors.margins: 4
                            visible: cell.visualFocus
                            color: "transparent"
                            border.width: 2
                            border.color: Theme.focus
                        }
                    }
                    contentItem: Text {
                        textFormat: Text.PlainText
                        text: cell.modelData.label
                        elide: Text.ElideRight
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                        color: cell.on ? Theme.primaryFillInk : Theme.ink2
                        font: Theme.type.control
                    }
                    HoverHandler { cursorShape: Qt.PointingHandCursor }
                }
            }
        }
    }

    Flow {
        visible: (root.chosen?.desc ?? "").length > 0 || (root.chosen?.badge ?? "").length > 0 || root.busy
        width: parent.width
        spacing: 12
        Rectangle {
            visible: (root.chosen?.badge ?? "").length > 0
            width: badge.implicitWidth + 12
            height: 20
            color: "transparent"
            border.width: 1
            border.color: Theme.lineStrong
            Text {
                id: badge
                anchors.centerIn: parent
                textFormat: Text.PlainText
                text: root.chosen?.badge ?? ""
                color: Theme.ink2
                font: Theme.type.silk
            }
        }
        Text {
            width: Math.min(implicitWidth, root.width)
            topPadding: 2
            textFormat: Text.PlainText
            text: root.chosen?.desc ?? ""
            wrapMode: Text.Wrap
            color: Theme.ink2
            font: Theme.type.description
        }
        LedLabel {
            visible: root.busy
            text: qsTr("Saving…")
            ledState: "idle"
        }
    }

    function step(by) {
        const at = options.findIndex(option => option.value === value)
        const next = options[(at + by + options.length) % options.length]
        if (next)
            picked(next.value)
    }
}
