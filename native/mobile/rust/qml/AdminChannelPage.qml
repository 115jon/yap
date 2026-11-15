pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// A channel's settings on the phone (the desktop's ChannelSettings, on the
// shared Yap.Ui ChannelDraft): "Text channel, created …", the name (a text
// channel's reads lowercase with dashes), the topic with its count, public
// sharing, then Save changes; Permissions opens the channel's overrides;
// Delete channel asks first, and the page closes once it is gone.
AdminPage {
    id: root

    property string channelId: ""

    readonly property ChannelDraft draft: ChannelDraft { model: root.model }
    readonly property var deleting: model.acts["delete-channel"] ?? { busy: false, error: "", done: 0 }

    title: channel ? channel.label : qsTr("Channel settings")
    watchChannel: true
    guarding: draft.dirty && !draft.saving
    discardBody: qsTr("Your channel edits have not been saved.")

    Component.onCompleted: {
        Admin.clear("delete-channel")
        Admin.openChannel(channelId)
    }
    Component.onDestruction: Admin.closeChannel()

    overlays: [
        ConfirmSheet {
            id: deleteSheet
            anchors.fill: parent
            title: qsTr("Delete channel")
            body: root.channel ? qsTr("%1 and all of its messages will be deleted. This can't be undone.").arg(root.channel.label) : ""
            confirmText: qsTr("Delete channel")
            danger: true
            onConfirmed: Admin.deleteChannel(root.channelId)
        }
    ]

    AdminSection {
        visible: root.channel !== null
        title: qsTr("Channel details")
        description: root.channel ? root.channel.created : ""

        Column {
            width: parent.width
            spacing: 20
            AdminField {
                page: root
                label: qsTr("Channel name")
                hint: root.draft.nameHint
                error: root.draft.nameError
                text: root.draft.name
                maximumLength: 100
                inputMethodHints: Qt.ImhNoPredictiveText
                onEdited: text => root.draft.nameEdited(text)
                onAccepted: root.draft.save()
            }
            Column {
                id: topicBlock
                width: parent.width
                spacing: 6
                Text {
                    textFormat: Text.PlainText
                    text: qsTr("Topic")
                    color: Theme.ink1
                    font: Theme.type.labelStrong
                    Accessible.ignored: true
                }
                TextArea {
                    id: topicField
                    width: parent.width
                    // Follows the draft without a binding; the draft takes
                    // the word being composed too (`live`).
                    readonly property string live: text + preeditText
                    Component.onCompleted: text = root.draft.description
                    Connections {
                        target: root.draft
                        function onDescriptionChanged() {
                            if (topicField.live !== root.draft.description)
                                topicField.text = root.draft.description
                        }
                    }
                    placeholderText: qsTr("What this channel is for")
                    wrapMode: TextEdit.Wrap
                    leftPadding: 14
                    rightPadding: 14
                    topPadding: 12
                    bottomPadding: 12
                    color: Theme.ink1
                    placeholderTextColor: Theme.ink3
                    selectionColor: Theme.selection
                    selectedTextColor: Theme.ink1
                    font: Theme.type.message
                    Accessible.name: qsTr("Topic")
                    background: Rectangle {
                        implicitHeight: 96
                        radius: Theme.radius
                        color: Theme.ply2
                        border.width: topicField.activeFocus ? 2 : 1
                        border.color: topicField.activeFocus ? Theme.ink1 : Theme.lineField
                    }
                    onTextChanged: {
                        if (text.length > root.draft.topicMax) {
                            text = text.slice(0, root.draft.topicMax)
                            cursorPosition = text.length
                        }
                    }
                    onLiveChanged: {
                        if (live !== root.draft.description)
                            root.draft.description = live
                    }
                    onActiveFocusChanged: if (activeFocus) Qt.callLater(root.reveal, topicBlock)
                }
                Text {
                    width: parent.width
                    horizontalAlignment: Text.AlignRight
                    textFormat: Text.PlainText
                    text: qsTr("%1 / %2").arg(root.draft.description.length).arg(root.draft.topicMax)
                    color: Theme.ink3
                    font: Theme.type.numSm
                }
            }
        }
    }

    AdminSection {
        visible: root.channel !== null
        title: qsTr("Public sharing")
        description: qsTr("Whether people can share messages from this channel with a public link.")

        Repeater {
            model: root.draft.shareIds
            delegate: AdminRow {
                id: shareRow
                required property string modelData
                required property int index
                text: root.draft.shareNames[index]
                chevron: false
                Accessible.role: Accessible.RadioButton
                Accessible.checked: root.draft.shares === modelData
                onOpened: root.draft.shares = modelData
                Icon {
                    visible: root.draft.shares === shareRow.modelData
                    name: "check"
                    size: Theme.iconLg
                    color: Theme.ink1
                }
            }
        }
    }

    Column {
        visible: root.channel !== null
        width: parent.width
        spacing: 8
        AdminNote {
            text: root.draft.saveError
        }
        KitButton {
            width: parent.width
            size: "lg"
            variant: "primary"
            text: qsTr("Save changes")
            enabled: root.draft.dirty
            loading: root.draft.saving
            onClicked: {
                Qt.inputMethod.commit()
                root.draft.save()
            }
        }
    }

    AdminSection {
        visible: root.channel !== null
        title: qsTr("Permissions")

        AdminRow {
            iconName: "shield"
            text: qsTr("Permission overrides")
            hint: qsTr("Change what roles and members can do in this channel")
            onOpened: Router.open("AdminOverridesPage")
        }
    }

    AdminSection {
        visible: root.channel !== null
        title: qsTr("Delete channel")

        AdminNote {
            bottomPadding: 8
            text: root.deleting.error
        }
        KitButton {
            width: parent.width
            size: "lg"
            variant: "danger"
            iconName: "trash"
            text: qsTr("Delete channel")
            loading: root.deleting.busy
            onClicked: {
                Admin.clear("delete-channel")
                deleteSheet.open()
            }
        }
    }
}
