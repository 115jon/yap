import QtQuick
import QtQuick.Templates as T
import Yap.Chrome
import com.yap.native
import Yap.Ui

// The menu behind a mic, headphone or camera caret (AudioDeviceMenu.tsx):
// the title, the devices as radio rows (the system default first), the
// output volume for speakers, then the settings row. It opens above its
// caret and stays anchored there as the list fills in.
MenuPanel {
    id: root

    // "input", "output" or "camera".
    required property string mode
    readonly property bool input: mode === "input"
    readonly property bool camera: mode === "camera"
    readonly property list<string> ids: camera ? ["default"].concat(Camera.deviceIds)
        : input ? Audio.inputIds : Audio.outputIds
    readonly property list<string> names: camera ? [""].concat(Camera.deviceNames)
        : input ? Audio.inputNames : Audio.outputNames
    readonly property string current: camera ? Audio.cameraDevice
        : input ? Audio.inputDevice : Audio.outputDevice
    readonly property string defaultName: camera ? qsTr("Default camera")
        : input ? qsTr("Default microphone") : qsTr("Default speaker")

    signal voiceSettings()

    // Above its caret: it grows up from there.
    transformOrigin: T.Popup.BottomLeft
    function openAbove(anchor) {
        parent = anchor
        x = 0
        y = Qt.binding(() => -root.height - 8)
        open()
    }

    function pick(id, name) {
        if (camera)
            Audio.selectCamera(id, id === "default" ? "" : name)
        else if (input)
            Audio.selectInput(id)
        else
            Audio.selectOutput(id)
    }

    implicitWidth: 240
    onAboutToShow: {
        if (!camera)
            Audio.refreshDevices()
    }

    MenuLabel {
        text: root.camera ? qsTr("Camera") : root.input ? qsTr("Input settings") : qsTr("Output settings")
    }

    Instantiator {
        model: root.ids
        delegate: MenuChoice {
            required property int index
            required property string modelData
            readonly property string name: root.names[index] ?? ""
            text: index > 0 ? name
                : name.length > 0 ? name
                : root.defaultName
            hint: index === 0 ? qsTr("Default") : ""
            selected: modelData === root.current || (index === 0 && root.ids.indexOf(root.current) < 0)
            onTriggered: root.pick(modelData, name)
        }
        onObjectAdded: (index, object) => root.insertItem(index + 1, object)
        onObjectRemoved: (index, object) => root.removeItem(object)
    }

    MenuRule {}
    VolumeSlider {
        visible: root.mode === "output"
        height: visible ? implicitHeight : 0
        label: qsTr("Output volume")
        value: Audio.outputVolume
        onMoved: Audio.changeOutputVolume(Math.round(value))
    }
    MenuRule {
        visible: root.mode === "output"
        height: visible ? implicitHeight : 0
    }
    MenuRow {
        text: root.camera ? qsTr("Camera settings") : qsTr("Voice settings")
        glyph: root.camera ? "camera" : "settings"
        onTriggered: root.voiceSettings()
    }
}
