import QtQuick
import QtQuick.Templates as T
import com.yap.native

// Stream quality (StreamQualityControl.tsx): a Resolution row and a Frame
// rate row of chips over one preset list (screen-share-support.ts), and
// the chosen preset's one-line hint. `value` and `picked` use preset ids
// ("1080p60"). Chips sit in a tint-1 well; the chosen one is accent tint
// with accent ink. A frame rate the resolution lacks is disabled, so the
// rows keep their shape.
Column {
    id: root

    property string value: "720p30"
    property bool showHint: true

    signal picked(string id)

    readonly property var presets: [
        { id: "720p30", label: "720p", fps: 30, desc: qsTr("Light on bandwidth") },
        { id: "720p60", label: "720p", fps: 60, desc: qsTr("Smooth motion") },
        { id: "1080p30", label: "1080p", fps: 30, desc: qsTr("Sharp text and apps") },
        { id: "1080p60", label: "1080p", fps: 60, desc: qsTr("Sharp and smooth") },
        { id: "1440p30", label: "1440p", fps: 30, desc: qsTr("Extra detail") },
        { id: "1440p60", label: "1440p", fps: 60, desc: qsTr("Extra detail, smooth") },
        { id: "4k30", label: "4K", fps: 30, desc: qsTr("Most detail") },
        { id: "4k60", label: "4K", fps: 60, desc: qsTr("Most detail, smooth") },
    ]
    readonly property var selected: presets.find(p => p.id === value) ?? presets[0]
    readonly property var resolutions: ["720p", "1080p", "1440p", "4K"]
    readonly property var rates: [30, 60]

    function pickResolution(label) {
        const next = presets.find(p => p.label === label && p.fps === selected.fps)
            ?? presets.find(p => p.label === label)
        if (next)
            picked(next.id)
    }
    function pickRate(fps) {
        const next = presets.find(p => p.label === selected.label && p.fps === fps)
        if (next)
            picked(next.id)
    }

    spacing: 8

    Row {
        spacing: 12
        Text {
            id: resolutionLabel
            anchors.verticalCenter: parent.verticalCenter
            width: 88
            text: qsTr("Resolution")
            color: Theme.ink3
            font: Theme.type.captionStrong
        }
        Well {
            label: resolutionLabel.text
            Repeater {
                model: root.resolutions
                delegate: Chip {
                    required property string modelData
                    text: modelData
                    checked: root.selected.label === modelData
                    onClicked: root.pickResolution(modelData)
                }
            }
        }
    }

    Row {
        spacing: 12
        Text {
            id: rateLabel
            anchors.verticalCenter: parent.verticalCenter
            width: 88
            text: qsTr("Frame rate")
            color: Theme.ink3
            font: Theme.type.captionStrong
        }
        Well {
            label: rateLabel.text
            Repeater {
                model: root.rates
                delegate: Chip {
                    required property int modelData
                    text: qsTr("%1 fps").arg(modelData)
                    checked: root.selected.fps === modelData
                    enabled: root.presets.some(p => p.label === root.selected.label && p.fps === modelData)
                    onClicked: root.pickRate(modelData)
                }
            }
        }
    }

    Text {
        visible: root.showHint
        leftPadding: 100
        text: root.selected.desc
        color: Theme.ink3
        font: Theme.type.caption
    }

    // A row's chips: a tint-1 well, 2 px in, 2 px between.
    component Well: Rectangle {
        property string label
        default property alias chips: chipRow.data
        width: chipRow.implicitWidth + 4
        height: chipRow.implicitHeight + 4
        radius: Theme.radius
        color: Theme.tint1
        Accessible.role: Accessible.Grouping
        Accessible.name: label
        Row {
            id: chipRow
            x: 2
            y: 2
            spacing: 2
        }
    }

    component Chip: T.AbstractButton {
        id: chip
        implicitWidth: Math.max(48, label.implicitWidth + 20)
        implicitHeight: 28
        focusPolicy: Qt.StrongFocus
        checkable: false
        opacity: enabled ? 1 : 0.45
        Accessible.role: Accessible.RadioButton
        Accessible.checkable: true
        Accessible.name: text
        contentItem: Text {
            id: label
            text: chip.text
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            color: chip.checked ? Theme.accentInk : chip.hovered && chip.enabled ? Theme.ink1 : Theme.ink2
            font.family: chip.checked ? "Yap Geist SemiBold" : "Yap Geist Medium"
            font.pixelSize: 14
            font.weight: chip.checked ? Font.DemiBold : Font.Medium
        }
        background: Rectangle {
            radius: Theme.radius
            color: chip.checked ? Theme.accentTint : "transparent"
            border.width: chip.visualFocus ? 2 : 0
            border.color: Theme.focus
            Behavior on color { ColorAnimation { duration: Theme.dur1 } }
        }
    }
}
