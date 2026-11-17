import QtQuick
import Yap.Ui

// Custom sounds (experimental) for both apps (CustomSoundsCard.tsx): the
// imported files over the built-in UI sounds, kept on this device
// (`CustomSounds.state`). Not visual: the desktop's card and the phone's
// section draw it their own way. `hidden` drops sounds an app never plays
// itself (the phone rings with Android's ringtone).
QtObject {
    id: root

    // Sound ids this app leaves out.
    property var hidden: []

    readonly property var model: {
        try {
            return JSON.parse(CustomSounds.state || "{}")
        } catch (error) {
            return {}
        }
    }
    readonly property var rows: (model.rows ?? []).filter(row => !root.hidden.includes(row.id))
    // Any sound has a file.
    readonly property bool any: model.any === true
    readonly property bool bulkBusy: model.bulkBusy === true
    readonly property string bulkLine: model.bulkLine ?? ""
    readonly property var bulkFailures: model.bulkFailures ?? []

    // What plays for `row`: its file and length, or the built-in sound.
    function describe(row) {
        return row.custom ? row.name + " · " + row.seconds : qsTr("Built-in · %1").arg(row.stem)
    }
    // What Reset all does.
    readonly property string resetAllPrompt: qsTr("Every imported file is deleted from this device. All sounds go back to the built-in ones.")

    function preview(id) {
        CustomSounds.preview(id)
    }
    function importFile(id, path) {
        CustomSounds.importFile(id, path)
    }
    function importFiles(paths) {
        CustomSounds.importFiles(paths)
    }
    function reset(id) {
        CustomSounds.reset(id)
    }
    function resetAll() {
        CustomSounds.resetAll()
    }
}
