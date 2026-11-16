pragma Singleton
import QtQuick
import Yap.Mobile
import Yap.Ui

// Files saved into Downloads/Yap (MediaPage's Files): the core downloads
// the file into the cache (with the session for the API's own files), then
// Native copies it into Downloads through MediaStore and deletes the
// download. It lives here, not on the page, so a save still lands when the
// page closes before the download does.
//
// - save(key, url, name): starts one; false when that key is already
//   saving or there is nowhere to download to.
// - `saving`: the keys under way; `finished(key, ok, text)` says how one
//   ended, in words for the page's note.
QtObject {
    id: saves

    // Cache path -> key, for the saves under way.
    property var byPath: ({})
    property var saving: ({})

    signal finished(string key, bool ok, string text)

    function save(key, url, name) {
        if (saves.saving[key] !== undefined)
            return false
        const path = Native.saveTarget(name)
        if (path.length === 0)
            return false
        saves.byPath[path] = key
        const keys = Object.assign({}, saves.saving)
        keys[key] = path
        saves.saving = keys
        Chat.download(url, path, false)
        return true
    }
    function end(path, ok, text) {
        const key = saves.byPath[path]
        delete saves.byPath[path]
        const keys = Object.assign({}, saves.saving)
        delete keys[key]
        saves.saving = keys
        saves.finished(key, ok, text)
    }

    property Connections core: Connections {
        target: Chat
        function onSaved(path, open) {
            if (path in saves.byPath)
                Native.saveToDownloads(path, "")
        }
        // The core removes what it wrote of a failed download.
        function onDownloadFailed(path, text) {
            if (path in saves.byPath)
                saves.end(path, false, text)
        }
    }
    property Connections platform: Connections {
        target: Native
        function onImageSaved(path, result) {
            if (!(path in saves.byPath))
                return
            saves.end(path, result === 0, result === 0 ? qsTr("Saved to Downloads/Yap")
                : result === 2 ? qsTr("Allow storage access, then save again.")
                : result === 3 ? qsTr("Turn on Storage for Yap in Settings, then save again.")
                : qsTr("Couldn't save the file."))
        }
    }
}
