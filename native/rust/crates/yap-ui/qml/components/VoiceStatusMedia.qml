import QtQuick
import Yap.Ui

// A voice channel's status picture (the tree's `voiceStatusMedia`:
// `<url>#<type>;<w>x<h>`), read for the desktop's and the phone's status
// lines: its URL, type, whether it is a clip, and its size (at least 1).
// Parsed in one binding from `media` alone: split over several (the "#"
// position, then the URL from it) a new picture could be read with the
// old one's "#" position or type for a moment, and the picture or the
// clip player was handed a cut-off URL or the other kind's file.
QtObject {
    property string media

    readonly property var info: {
        const at = media.lastIndexOf("#")
        const tail = at >= 0 ? media.slice(at + 1) : ""
        const semi = tail.lastIndexOf(";")
        const size = semi >= 0 ? tail.slice(semi + 1).split("x") : []
        const type = semi >= 0 ? tail.slice(0, semi) : ""
        return {
            url: at >= 0 ? media.slice(0, at) : media,
            type: type,
            video: type.indexOf("video/") === 0,
            w: Math.max(1, Number(size[0]) || 1),
            h: Math.max(1, Number(size[1]) || 1)
        }
    }
}
