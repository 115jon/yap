pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// Channel settings → Overview (ChannelSettingsModal.tsx): Channel details,
// "Text channel, created …": the name (text channels show how it will read,
// lowercase with dashes), the topic with its count, and public sharing
// (the server's setting, on or off). A draft behind the unsaved bar.
Column {
    id: root

    required property var model

    readonly property var channel: model.channel
    readonly property var act: model.acts["channel"] ?? { busy: false, error: "", done: 0 }
    readonly property string nameError: model.acts["channel.name"]?.error ?? ""

    property string name: channel.name
    property string description: channel.description
    property string shares: channel.publicShares
    property int savedCount: 0

    readonly property string preview: Admin.sanitizeChannelName(name, channel.voice, false)
    readonly property string finalName: Admin.sanitizeChannelName(name, channel.voice, true)
    readonly property bool dirty: finalName !== channel.name || description !== channel.description || shares !== channel.publicShares
    readonly property bool saving: act.busy
    readonly property string saveError: act.error

    function reset() {
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

    spacing: 40
    Component.onCompleted: savedCount = act.done
    onActChanged: {
        if (act.done < savedCount)
            savedCount = act.done
        else if (act.done > savedCount) {
            savedCount = act.done
            name = channel.name
            description = channel.description
            shares = channel.publicShares
        }
    }

    SettingsCard {
        width: parent.width
        title: qsTr("Channel details")
        description: root.channel.created

        SettingsRow {
            custom: true
            layout: "label"
            label: qsTr("Channel name")
            description: root.channel.voice ? qsTr("Shown in the channel list as typed.")
                : root.preview !== root.name ? qsTr("Shown as #%1").arg(root.preview.length > 0 ? root.preview : qsTr("channel"))
                : qsTr("Lowercase, with dashes instead of spaces.")
            KitField {
                width: parent.width
                text: root.name
                maximumLength: 100
                invalid: root.nameError.length > 0
                Accessible.name: qsTr("Channel name")
                onTextEdited: {
                    root.name = text
                    Admin.clear("channel.name")
                }
                Keys.onReturnPressed: root.save()
            }
            footer: RowError { text: root.nameError }
        }
        SettingsRow {
            custom: true
            layout: "label"
            label: qsTr("Topic")
            TopicField {
                width: parent.width
                value: root.description
                max: 1024
                placeholder: qsTr("What this channel is for")
                accessibleName: qsTr("Topic")
                onEdited: value => root.description = value
            }
            // FieldRow's helper line: gap-1.5, the count at Num sm on a 20
            // line.
            footer: Text {
                width: parent.width
                topPadding: 8
                lineHeight: 20
                lineHeightMode: Text.FixedHeight
                horizontalAlignment: Text.AlignRight
                textFormat: Text.PlainText
                text: qsTr("%1 / %2").arg(root.description.length).arg(1024)
                color: Theme.ink3
                font: Theme.type.numSm
            }
        }
        SettingsRow {
            custom: true
            // layout="stack": the 40 px SettingsSelect trigger on the
            // trailing side, 240 wide.
            layout: "stack"
            label: qsTr("Public sharing")
            description: qsTr("Whether people can share messages from this channel with a public link.")
            SelectField {
                width: 240
                firstIsDefault: false
                text: qsTr("Public sharing")
                ids: ["inherit", "allow", "deny"]
                names: [qsTr("Use the server setting"), qsTr("Allow public links"), qsTr("Turn off public links")]
                current: root.shares
                onPicked: id => root.shares = id
            }
        }
    }
}
