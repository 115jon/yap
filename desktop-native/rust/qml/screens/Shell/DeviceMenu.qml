import QtQuick
import com.yap.native

// The menu behind a mic or headphone caret (AudioDeviceMenu.tsx): the
// title, the devices as radio rows (the system default first), the output
// volume for speakers, then "Voice settings". It opens above its caret and
// stays anchored there as the list fills in.
MenuPanel {
    id: root

    // "input" or "output".
    required property string mode
    readonly property bool input: mode === "input"
    readonly property list<string> ids: input ? Audio.inputIds : Audio.outputIds
    readonly property list<string> names: input ? Audio.inputNames : Audio.outputNames
    readonly property string current: input ? Audio.inputDevice : Audio.outputDevice

    signal voiceSettings()

    function openAbove(anchor) {
        parent = anchor
        x = 0
        y = Qt.binding(() => -root.height - 8)
        open()
    }

    implicitWidth: 240
    onAboutToShow: Audio.refreshDevices()

    MenuLabel { text: root.input ? qsTr("Input settings") : qsTr("Output settings") }

    Instantiator {
        model: root.ids
        delegate: MenuChoice {
            required property int index
            required property string modelData
            readonly property string name: root.names[index] ?? ""
            text: index > 0 ? name
                : name.length > 0 ? name
                : root.input ? qsTr("Default microphone") : qsTr("Default speaker")
            hint: index === 0 ? qsTr("Default") : ""
            selected: modelData === root.current || (index === 0 && root.ids.indexOf(root.current) < 0)
            onTriggered: root.input ? Audio.selectInput(modelData) : Audio.selectOutput(modelData)
        }
        onObjectAdded: (index, object) => root.insertItem(index + 1, object)
        onObjectRemoved: (index, object) => root.removeItem(object)
    }

    MenuRule {}
    VolumeSlider {
        visible: !root.input
        height: visible ? implicitHeight : 0
        label: qsTr("Output volume")
        value: Audio.outputVolume
        onMoved: Audio.changeOutputVolume(Math.round(value))
    }
    MenuRule {
        visible: !root.input
        height: visible ? implicitHeight : 0
    }
    MenuRow {
        text: qsTr("Voice settings")
        glyph: "settings"
        onTriggered: root.voiceSettings()
    }
}
