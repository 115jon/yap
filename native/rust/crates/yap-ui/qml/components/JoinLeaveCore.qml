import QtQuick
import QtQml.Models
import Yap.Ui

// Join and leave sounds in settings, for both apps (SoundboardPanel.tsx
// `TriggerRow` and its chooser). Not visual: the desktop's card and the
// phone's page each draw it their own way. The store owns the rules
// (`Apps.state`: `joins`, `leaves`, `catalogs`, `uploadServers`); this keeps
// that part of the state, replaced only when its content changes, the
// servers in the rail's order, the scope (All servers or one server's
// override), what the chooser offers for it and the rows' words.
QtObject {
    id: root

    // `*` (All servers) or a server id.
    property string scope: "*"

    // The state is parsed afresh on every change, and a new object would
    // rebuild every Repeater under it; this slice is replaced only when its
    // content differs (as FriendsView and SearchPanel do).
    readonly property var parsed: {
        try {
            return JSON.parse(Apps.state || "{}")
        } catch (error) {
            return {}
        }
    }
    property var s: ({})
    property string sKey: "{}"
    function sync() {
        const all = root.parsed
        const next = {
            joins: all.joins ?? {},
            leaves: all.leaves ?? {},
            catalogs: all.catalogs ?? {},
            uploadServers: all.uploadServers ?? [],
            uploading: all.uploading === true,
            error: all.soundError ?? ""
        }
        const key = JSON.stringify(next)
        if (key !== root.sKey) {
            root.sKey = key
            root.s = next
        }
    }
    onParsedChanged: sync()
    Component.onCompleted: sync()

    // The servers, in the rail's order.
    property var serverIds: []
    property var serverNames: []
    function collect() {
        const ids = [], names = []
        for (let i = 0; i < servers.count; i++) {
            const item = servers.objectAt(i)
            if (item) {
                ids.push(item.serverId)
                names.push(item.name)
            }
        }
        root.serverIds = ids
        root.serverNames = names
    }
    property Instantiator servers: Instantiator {
        model: ServerListModel
        delegate: QtObject {
            required property string serverId
            required property string name
        }
        onObjectAdded: Qt.callLater(root.collect)
        onObjectRemoved: Qt.callLater(root.collect)
    }

    function serverName(id) {
        return root.serverNames[root.serverIds.indexOf(id)] ?? ""
    }
    readonly property string scopeName: scope === "*" ? qsTr("All servers") : serverName(scope)
    readonly property string scopeDescription: scope === "*"
        ? qsTr("Set sounds for every server, or override them for one.")
        : qsTr("%1 uses your All servers sounds until you change them here.").arg(serverName(scope))

    // The servers whose sounds the chooser offers: every one for All
    // servers, else the scope's.
    readonly property var choiceServerIds: scope === "*" ? serverIds : [scope]
    readonly property var defaults: [
        { pick: "default:ping", name: qsTr("Ping"), emoji: "" },
        { pick: "default:pop", name: qsTr("Pop"), emoji: "" },
        { pick: "default:chime", name: qsTr("Chime"), emoji: "" },
        { pick: "default:tada", name: qsTr("Ta-da"), emoji: "" }
    ]
    // Each offered server's sounds as chooser entries, and whether we may
    // upload one to it.
    readonly property var sections: choiceServerIds.map(id => ({
        serverId: id,
        name: serverName(id),
        loaded: s.catalogs?.[id] !== undefined,
        canUpload: (s.uploadServers ?? []).includes(id),
        sounds: (s.catalogs?.[id] ?? []).map(sound => ({
            pick: "server:" + id + ":" + sound.id,
            name: sound.name,
            emoji: sound.emoji ?? ""
        }))
    }))
    // All of them in one list (the desktop's chips): the defaults, then
    // each server's.
    readonly property var choices: {
        const out = root.defaults.map(x => ({ pick: x.pick, name: x.name }))
        for (const section of root.sections)
            for (const sound of section.sounds)
                out.push({ pick: sound.pick, name: sound.emoji ? sound.emoji + " " + sound.name : sound.name })
        return out
    }
    readonly property bool uploading: s.uploading === true
    readonly property string error: s.error ?? ""

    // A row's state for `trigger` (join | leave) in the scope: enabled,
    // name, emoji, hasSound, inherited, pick; null before the state is in.
    function config(trigger) {
        return ((trigger === "join" ? s.joins : s.leaves) ?? {})[root.scope] ?? null
    }
    function action(trigger) {
        return trigger === "join" ? qsTr("join") : qsTr("leave")
    }
    function title(trigger) {
        return trigger === "join" ? qsTr("Join sound") : qsTr("Leave sound")
    }
    // The sound, and whether it comes from All servers.
    function describe(config) {
        return (config?.emoji ? config.emoji + " " : "") + (config?.name ?? qsTr("No sound chosen"))
            + (config?.inherited ? qsTr(" · From All servers") : "")
    }
    // What clearing it does.
    function clearPrompt(trigger, config) {
        return qsTr("%1 stops playing when you %2 a voice channel. You can pick a sound again any time.")
            .arg(config?.name ?? qsTr("This sound")).arg(action(trigger))
    }

    // The chooser opened: the offered servers' sounds load.
    function loadCatalogs() {
        for (const id of root.choiceServerIds)
            Apps.loadCatalog(id)
    }
    function preview(trigger) {
        Apps.previewTrigger(trigger, root.scope)
    }
    function previewPick(pick) {
        Apps.previewPick(pick)
    }
    function choose(trigger, pick) {
        Apps.setTrigger(trigger, root.scope, -1, pick, false)
    }
    function setEnabled(trigger, on) {
        Apps.setTrigger(trigger, root.scope, on ? 1 : 0, "", false)
    }
    function clear(trigger) {
        Apps.setTrigger(trigger, root.scope, -1, "", true)
    }
    // Upload the audio file at `path` to `serverId`, then make it the
    // `trigger` sound for the scope.
    function upload(trigger, serverId, path, name, emoji, volume) {
        Apps.uploadTrigger(serverId, trigger, root.scope, path, name, emoji, volume)
    }
    function clearError() {
        Apps.clearError()
    }
}
