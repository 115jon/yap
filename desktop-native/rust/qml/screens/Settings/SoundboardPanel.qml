pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// Soundboard (SoundboardPanel.tsx): the playback volume, kept on this
// device, and the join and leave sounds for All servers or one server,
// synced to the account.
Column {
    id: root

    // The state is parsed afresh on every change, and a new object would
    // rebuild every Repeater under it; this slice is replaced only when its
    // content differs (as FriendsView and SearchPanel do).
    readonly property var parsed: JSON.parse(Apps.state || "{}")
    property var s: ({})
    property string sKey: "{}"

    function syncS() {
        const next = root.soundboardOf(parsed)
        const key = JSON.stringify(next)
        if (key !== sKey) {
            sKey = key
            s = next
        }
    }
    onParsedChanged: syncS()
    Component.onCompleted: syncS()

    // The soundboard's part of the state: Listen Together and Daily Word
    // changes leave it (and its Repeaters) alone.
    function soundboardOf(all) {
        const next = Object.assign({}, all)
        delete next.listen
        delete next.wordle
        delete next.others
        delete next.activity
        delete next.playing
        return next
    }
    property string scope: "*"
    property var serverIds: []
    property var serverNames: []

    function collect() {
        const ids = [], names = []
        for (let i = 0; i < servers.count; i++) {
            const item = servers.itemAt(i)
            if (item) {
                ids.push(item.serverId)
                names.push(item.name)
            }
        }
        serverIds = ids
        serverNames = names
    }

    spacing: 40

    Repeater {
        id: servers
        model: ServerListModel
        delegate: Item {
            visible: false
            required property string serverId
            required property string name
        }
        onCountChanged: Qt.callLater(root.collect)
    }

    SettingsCard {
        width: parent.width
        sectionId: "soundboard-playback"
        SettingsRow {
            rowId: "soundboard-volume"
            layout: "block"
            RangeControl {
                width: parent.width
                value: Desktop.soundboardVolume
                accessibleName: SettingsNav.label("soundboard-volume")
                onMoved: value => Desktop.setVolume("soundboardVolume", value)
            }
        }
    }

    SettingsCard {
        width: parent.width
        sectionId: "join-leave-sounds"
        SettingsRow {
            custom: true
            layout: "inline"
            label: qsTr("Server")
            description: root.scope === "*" ? qsTr("Set sounds for every server, or override them for one.")
                : qsTr("%1 uses your All servers sounds until you change them here.")
                    .arg(root.serverNames[root.serverIds.indexOf(root.scope)] ?? "")
            SelectField {
                width: 220
                ids: ["*"].concat(root.serverIds)
                names: [qsTr("All servers")].concat(root.serverNames)
                current: root.scope
                firstIsDefault: false
                onPicked: id => root.scope = id
            }
        }
        TriggerRow {
            rowId: "join-sound"
            trigger: "join"
            scope: root.scope
            config: (root.s.joins ?? {})[root.scope] ?? null
            catalogs: root.s.catalogs ?? {}
            serverIds: root.scope === "*" ? root.serverIds : [root.scope]
        }
        TriggerRow {
            rowId: "leave-sound"
            trigger: "leave"
            scope: root.scope
            config: (root.s.leaves ?? {})[root.scope] ?? null
            catalogs: root.s.catalogs ?? {}
            serverIds: root.scope === "*" ? root.serverIds : [root.scope]
        }
    }
}
