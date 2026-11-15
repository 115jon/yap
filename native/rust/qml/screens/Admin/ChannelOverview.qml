pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// Channel settings → Overview (ChannelSettingsModal.tsx): Channel details,
// "Text channel, created …": the name (text channels show how it will read,
// lowercase with dashes), the topic with its count, and public sharing
// (the server's setting, on or off). A draft behind the unsaved bar.
Column {
    id: root

    required property var model

    readonly property var channel: model.channel
    // The draft (Yap.Ui ChannelDraft, the phone's too).
    readonly property ChannelDraft draft: ChannelDraft { model: root.model }
    readonly property string nameError: draft.nameError
    readonly property bool dirty: draft.dirty
    readonly property bool saving: draft.saving
    readonly property string saveError: draft.saveError

    function reset() { draft.reset() }
    function save() { draft.save() }

    spacing: 40

    SettingsCard {
        width: parent.width
        title: qsTr("Channel details")
        description: root.channel.created

        SettingsRow {
            custom: true
            layout: "label"
            label: qsTr("Channel name")
            description: root.draft.nameHint
            KitField {
                id: nameField
                width: parent.width
                text: root.draft.name
                maximumLength: 100
                invalid: root.nameError.length > 0
                Accessible.name: qsTr("Channel name")
                onTextEdited: root.draft.nameEdited(nameField.text)
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
                value: root.draft.description
                max: root.draft.topicMax
                placeholder: qsTr("What this channel is for")
                accessibleName: qsTr("Topic")
                onEdited: value => root.draft.description = value
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
                text: qsTr("%1 / %2").arg(root.draft.description.length).arg(root.draft.topicMax)
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
                ids: root.draft.shareIds
                names: root.draft.shareNames
                current: root.draft.shares
                onPicked: id => root.draft.shares = id
            }
        }
    }
}
