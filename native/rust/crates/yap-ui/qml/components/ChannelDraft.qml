import QtQuick
import Yap.Ui

// Channel settings → Overview's draft for both apps
// (ChannelSettingsModal.tsx): the desktop's ChannelOverview and the
// phone's AdminChannelPage draw it their own way. The name (a text
// channel's reads lowercase with dashes: `preview`), the topic and public
// sharing (`inherit`, `allow` or `deny`) are a draft until `save()`; a
// save that went through makes the channel the draft again.
QtObject {
    id: draft

    property var model: JSON.parse(Admin.state)

    readonly property var channel: model.channel
    readonly property var act: model.acts["channel"] ?? { busy: false, error: "", done: 0 }
    readonly property string nameError: model.acts["channel.name"]?.error ?? ""

    property string name: channel ? channel.name : ""
    property string description: channel ? channel.description : ""
    property string shares: channel ? channel.publicShares : "inherit"
    property int savedCount: 0

    readonly property bool voice: channel ? channel.voice : false
    readonly property string preview: Admin.sanitizeChannelName(name, voice, false)
    readonly property string finalName: Admin.sanitizeChannelName(name, voice, true)
    readonly property bool dirty: channel !== null && (finalName !== channel.name || description !== channel.description || shares !== channel.publicShares)
    readonly property bool saving: act.busy
    readonly property string saveError: act.error
    readonly property int topicMax: 1024
    // The name field's hint: how a text channel's name will read.
    readonly property string nameHint: voice ? qsTr("Shown in the channel list as typed.")
        : preview !== name ? qsTr("Shown as #%1").arg(preview.length > 0 ? preview : qsTr("channel"))
        : qsTr("Lowercase, with dashes instead of spaces.")
    readonly property var shareIds: ["inherit", "allow", "deny"]
    readonly property var shareNames: [qsTr("Use the server setting"), qsTr("Allow public links"), qsTr("Turn off public links")]

    function reset() {
        if (!channel)
            return
        name = channel.name
        description = channel.description
        shares = channel.publicShares
        Admin.clear("channel")
        Admin.clear("channel.name")
    }
    function save() {
        if (dirty && !saving)
            Admin.saveChannel(name, description, shares)
    }
    function nameEdited(text) {
        name = text
        Admin.clear("channel.name")
    }

    Component.onCompleted: savedCount = act.done
    onActChanged: {
        if (act.done < savedCount)
            savedCount = act.done
        else if (act.done > savedCount) {
            savedCount = act.done
            if (!channel)
                return
            name = channel.name
            description = channel.description
            shares = channel.publicShares
        }
    }
}
