import QtQuick
import Yap.Ui

// What the soundboard picker does, for both apps (SoundboardPicker.tsx).
// Not visual: the desktop's popover and the phone's sheet each draw it
// their own way over the same lists. The store owns the rules
// (`Apps.state`); this keeps the soundboard's part of that state, and the
// MyInstants and radio searches apart from it, each replaced only when its
// content changes, so typing (which changes only the searches) and Listen
// Together's clock do not rebuild every tile. It filters the lists by the
// search text and says which sections show (a DM call has no custom
// sounds, MyInstants or radio).
QtObject {
    id: root

    // The search field's text, as typed.
    property string query: ""
    // The search list `searchState` reads: `myinstants` or `radio`.
    property string searchKind: "myinstants"

    // The state is parsed afresh on every change, and a new object would
    // rebuild every Repeater under it; these slices are replaced only when
    // their content differs (as FriendsView and SearchPanel do).
    readonly property var parsed: {
        try {
            return JSON.parse(Apps.state || "{}")
        } catch (error) {
            return {}
        }
    }
    property var s: ({})
    property string sKey: "{}"
    // The MyInstants and radio searches.
    property var found: ({})
    property string foundKey: "{}"

    function sync() {
        const next = root.soundboardOf(root.parsed)
        const key = JSON.stringify(next)
        if (key !== root.sKey) {
            root.sKey = key
            root.s = next
        }
        const nextFound = { instants: root.parsed.instants ?? {}, radio: root.parsed.radio ?? {} }
        const foundText = JSON.stringify(nextFound)
        if (foundText !== root.foundKey) {
            root.foundKey = foundText
            root.found = nextFound
        }
    }
    onParsedChanged: sync()
    Component.onCompleted: sync()

    // The soundboard's part of the state: Listen Together, Daily Word and
    // the searches leave it (and its Repeaters) alone.
    function soundboardOf(all) {
        const next = Object.assign({}, all)
        delete next.instants
        delete next.radio
        delete next.listen
        delete next.wordle
        delete next.others
        delete next.activity
        delete next.catalogs
        delete next.joins
        delete next.leaves
        return next
    }

    readonly property string filter: query.trim().toLowerCase()
    readonly property bool searching: filter.length > 0
    readonly property var searchState: searchKind === "radio" ? (found.radio ?? {}) : (found.instants ?? {})
    // `isDirectMessageScope`: no custom sounds, MyInstants or radio there.
    readonly property bool dmCall: s.inCall === true && (s.serverId ?? "") === ""
    // Custom sounds kept on this device: the web shows them only away from
    // a server; native keeps any saved for this server reachable.
    readonly property bool showCustom: !dmCall && (s.local ?? []).length > 0
    readonly property var favorites: shown(s.favorites)
    readonly property var defaults: shown(s.defaults)
    readonly property var customs: showCustom ? shown(s.local) : []
    // Each server's section: the call's server, or in a DM call every
    // member server, in the rail's order.
    readonly property var sections: s.sections ?? []
    readonly property var playing: s.playing ?? []
    readonly property bool loading: s.loadingSounds === true
    readonly property bool canUpload: s.canUpload === true
    readonly property bool uploading: s.uploading === true
    readonly property string error: s.soundError ?? ""
    // Send: how loud our clips play for the room, 0..1.
    readonly property real sendVolume: s.sendVolume ?? 0.8
    // The sections in the order a list shows them (a rail's scroll-spy).
    readonly property var sectionOrder: ["favorites"].concat(sections.map(x => "server:" + x.serverId), ["custom", "defaults"])

    // `list`, filtered by the search text.
    function shown(list) {
        return (list ?? []).filter(x => root.filter.length === 0 || x.name.toLowerCase().includes(root.filter))
    }
    // A server section's sounds under the search, and whether it leads with
    // Add sound (the call's own server, not while searching).
    function sectionSounds(section) {
        return root.shown(section.sounds)
    }
    function sectionCanAdd(section) {
        return section.own === true && !root.searching && root.canUpload
    }

    // The picker opened: the call server's sounds and the favourites load.
    function load() {
        Apps.loadSounds()
        Apps.loadFavorites()
    }
    // A MyInstants or radio search (empty: trending, or the top stations).
    function search(kind, text) {
        if (kind === "radio")
            Apps.searchRadio(text)
        else
            Apps.searchInstants(text)
    }

    function play(sound) {
        Apps.play(sound.id)
    }
    function preview(sound) {
        Apps.preview(sound.id)
    }
    function toggleFavorite(sound) {
        Apps.toggleFavorite(sound.id)
    }
    function remove(sound) {
        Apps.deleteSound(sound.id)
    }
    function setSendVolume(volume) {
        Apps.setSendVolume(volume)
    }
    function clearError() {
        Apps.clearError()
    }

    // What deleting `sound` asks (DeleteSoundDialog.tsx).
    function deletePrompt(sound) {
        return sound?.kind === "custom"
            ? qsTr("Delete %1 from this device?").arg(sound?.name ?? "")
            : qsTr("Delete %1? Everyone loses it.").arg(sound?.name ?? "")
    }
}
