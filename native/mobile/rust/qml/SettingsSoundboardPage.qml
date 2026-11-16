pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Settings > Soundboard: how loud the soundboard plays on this phone
// (Apps.soundboardVolume, 0..100, where the app keeps one), and the join
// and leave sounds (SoundboardPanel.tsx over Yap.Ui's JoinLeaveCore, as the
// desktop's panel): for All servers or one server's override, picked in a
// sheet. Each sound's row shows what plays, Preview and its switch; a tap
// opens the chooser (Yap's sounds and the servers' soundboards, each with
// Preview), where a server we may upload to offers Add a sound (the files
// picker on audio, then the soundboard's form) and the sound clears behind
// a confirm.
SettingsFrame {
    id: root

    property int boardVolume: Apps.soundboardVolume()

    // The row whose chooser is open: join | leave.
    property string trigger: "join"
    readonly property var triggerConfig: core.config(trigger)
    // Add a sound: the server it goes to, this page asked for the picker,
    // the picked copy the form uploads, and the uploaded copy, dropped once
    // the upload is over.
    property string uploadServer: ""
    property bool picking: false
    property string path: ""
    property string sentPath: ""
    property string notice: ""

    title: qsTr("Soundboard")

    JoinLeaveCore {
        id: core
    }
    // An in-call soundboard error is not this page's.
    Component.onCompleted: core.clearError()

    function openChooser(which) {
        Native.haptic(Native.Tick)
        root.trigger = which
        core.clearError()
        root.notice = ""
        core.loadCatalogs()
        chooserFlick.contentY = 0
        chooser.open()
    }
    function startUpload(serverId) {
        Native.haptic(Native.Tick)
        root.uploadServer = serverId
        root.notice = ""
        root.picking = true
        Native.pickAudio(50 * 1024 * 1024)
    }
    Connections {
        target: Native
        function onMediaPicked(paths, failed) {
            if (!root.picking)
                return
            root.picking = false
            if (paths.length > 0) {
                root.path = paths[0]
                form.reset(root.path.split("/").pop().replace(/\.[^.]+$/, ""), "", 100)
                formFlick.contentY = 0
                chooser.close()
                uploadSheet.open()
            } else if (failed > 0) {
                root.notice = qsTr("That file couldn't be read. Try another one.")
            }
        }
    }
    function dropPicked() {
        if (root.path.length > 0)
            Native.discardPicked(root.path)
        root.path = ""
    }
    readonly property bool uploading: core.uploading
    onUploadingChanged: {
        if (!uploading && sentPath.length > 0) {
            Native.discardPicked(sentPath)
            sentPath = ""
        }
    }

    overlays: [
        // All servers, or one server's override.
        BottomSheet {
            id: scopeSheet
            anchors.fill: parent
            title: qsTr("Server")
            contentHeight: Math.min(scopeColumn.implicitHeight, 2000)
            flickable: scopeFlick
            Flickable {
                id: scopeFlick
                anchors.fill: parent
                contentHeight: scopeColumn.implicitHeight
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                Column {
                    id: scopeColumn
                    width: scopeFlick.width
                    bottomPadding: 8
                    Repeater {
                        model: ["*"].concat(core.serverIds)
                        delegate: PickRow {
                            required property string modelData
                            text: modelData === "*" ? qsTr("All servers") : core.serverName(modelData)
                            chosen: core.scope === modelData
                            onPicked: {
                                core.scope = modelData
                                scopeSheet.close()
                            }
                        }
                    }
                }
            }
        },

        // Yap's sounds and the servers' soundboards for the row's sound.
        BottomSheet {
            id: chooser
            anchors.fill: parent
            title: core.title(root.trigger)
            contentHeight: chooserColumn.implicitHeight
            flickable: chooserFlick
            onClosed: root.picking = false
            Flickable {
                id: chooserFlick
                anchors.fill: parent
                contentHeight: chooserColumn.implicitHeight
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                Column {
                    id: chooserColumn
                    width: chooserFlick.width
                    bottomPadding: 8
                    GroupLabel {
                        text: qsTr("Yap sounds")
                    }
                    Repeater {
                        model: core.defaults
                        delegate: PickRow {
                            required property var modelData
                            text: modelData.name
                            pick: modelData.pick
                            chosen: root.triggerConfig?.pick === modelData.pick
                            onPicked: root.choose(modelData.pick)
                        }
                    }
                    Repeater {
                        model: chooser.up ? core.sections : []
                        delegate: Column {
                            id: section
                            required property var modelData
                            width: chooserColumn.width
                            GroupLabel {
                                text: section.modelData.name
                            }
                            SheetRow {
                                visible: section.modelData.canUpload
                                text: qsTr("Add a sound")
                                iconName: "upload"
                                onTriggered: root.startUpload(section.modelData.serverId)
                            }
                            Note {
                                visible: !section.modelData.loaded
                                text: qsTr("Loading sounds…")
                            }
                            Note {
                                visible: section.modelData.loaded && section.modelData.sounds.length === 0
                                text: qsTr("No sounds on this server yet.")
                            }
                            Repeater {
                                model: section.modelData.sounds
                                delegate: PickRow {
                                    required property var modelData
                                    text: modelData.emoji.length > 0 ? modelData.emoji + " " + modelData.name : modelData.name
                                    pick: modelData.pick
                                    chosen: root.triggerConfig?.pick === modelData.pick
                                    onPicked: root.choose(modelData.pick)
                                }
                            }
                        }
                    }
                    SettingsErrorText {
                        leftPadding: 16
                        rightPadding: 16
                        text: root.notice
                    }
                    SheetRow {
                        visible: root.triggerConfig?.hasSound === true && root.triggerConfig?.inherited !== true
                        text: root.trigger === "join" ? qsTr("Clear join sound") : qsTr("Clear leave sound")
                        iconName: "trash"
                        danger: true
                        onTriggered: {
                            chooser.close()
                            clearSheet.open()
                        }
                    }
                }
            }
        },

        // Add a sound: the soundboard's form, then the upload, which then
        // becomes the row's sound.
        BottomSheet {
            id: uploadSheet
            anchors.fill: parent
            title: qsTr("Add sound")
            contentHeight: form.height + 16
            flickable: formFlick
            onClosed: root.dropPicked()
            Flickable {
                id: formFlick
                anchors.fill: parent
                contentHeight: form.height + 16
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                SoundForm {
                    id: form
                    x: 16
                    y: 4
                    width: formFlick.width - 32
                    path: root.path
                    busy: core.uploading
                    onSubmitted: (name, emoji, volume) => {
                        Native.haptic(Native.Confirm)
                        Qt.inputMethod.hide()
                        core.upload(root.trigger, root.uploadServer, root.path, name, emoji, volume)
                        // The core reads the file as it uploads: the copy
                        // goes after.
                        root.sentPath = root.path
                        root.path = ""
                        uploadSheet.close()
                    }
                    onCancelled: uploadSheet.close()
                    onTooBigAction: which => {
                        uploadSheet.close()
                        if (which === "choose" || which === "pick")
                            root.startUpload(root.uploadServer)
                    }
                }
            }
        },

        ConfirmSheet {
            id: clearSheet
            anchors.fill: parent
            title: root.trigger === "join" ? qsTr("Clear join sound?") : qsTr("Clear leave sound?")
            body: core.clearPrompt(root.trigger, root.triggerConfig)
            confirmText: qsTr("Clear sound")
            danger: true
            onConfirmed: {
                Native.haptic(Native.Confirm)
                core.clear(root.trigger)
            }
        }
    ]

    function choose(pick) {
        Native.haptic(Native.Tick)
        core.choose(root.trigger, pick)
        chooser.close()
    }

    SettingsSection {
        visible: root.boardVolume >= 0
        number: 1
        title: qsTr("Playback")
        SettingsVolumeRow {
            text: qsTr("Soundboard volume")
            description: qsTr("How loud sounds from the soundboard play on this phone.")
            value: Math.max(0, root.boardVolume)
            onCommitted: value => {
                Apps.setSoundboardVolume(value)
                root.boardVolume = value
            }
        }
    }

    SettingsSection {
        number: root.boardVolume >= 0 ? 2 : 1
        title: qsTr("Join and leave sounds")
        AdminRow {
            iconName: "server"
            text: qsTr("Server")
            hint: core.scopeName
            onOpened: scopeSheet.open()
        }
        Text {
            width: parent.width
            bottomPadding: 8
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            text: core.scopeDescription
            color: Theme.ink3
            font: Theme.type.caption
        }
        TriggerRow {
            trigger: "join"
        }
        TriggerRow {
            trigger: "leave"
        }
        Text {
            width: parent.width
            visible: core.uploading
            topPadding: 4
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            text: qsTr("Uploading your sound…")
            color: Theme.ink3
            font: Theme.type.caption
        }
        SettingsErrorText {
            text: core.error.length > 0 ? core.error : chooser.up ? "" : root.notice
        }
    }

    // A join or leave sound: what plays (and whether it comes from All
    // servers), Preview and the switch; a tap opens the chooser.
    component TriggerRow: AdminRow {
        id: row
        required property string trigger
        readonly property var config: core.config(trigger)
        text: core.title(trigger)
        hint: core.describe(config)
        onOpened: root.openChooser(row.trigger)
        IconKey {
            anchors.verticalCenter: parent.verticalCenter
            iconName: "play"
            tone: "ghost"
            enabled: row.config?.hasSound === true
            opacity: enabled ? 1 : 0.45
            tip: row.trigger === "join" ? qsTr("Preview join sound") : qsTr("Preview leave sound")
            Accessible.name: tip
            onClicked: {
                Native.haptic(Native.Tick)
                core.preview(row.trigger)
            }
        }
        SettingSwitch {
            anchors.verticalCenter: parent.verticalCenter
            checked: row.config?.enabled === true
            Accessible.name: qsTr("Play a sound when you %1").arg(core.action(row.trigger))
            onClicked: {
                Native.haptic(Native.Tick)
                core.setEnabled(row.trigger, !checked)
            }
        }
    }

    // One choice in a sheet: the name, Preview where it has a sound, the
    // check while chosen; the row picks it.
    component PickRow: Item {
        id: choice
        property string text
        property string pick: ""
        property bool chosen: false
        signal picked()
        width: parent ? parent.width : 0
        height: Math.max(Theme.controlLg, label.implicitHeight + 16)
        Accessible.role: Accessible.RadioButton
        Accessible.name: text
        Accessible.checkable: true
        Accessible.checked: chosen
        Accessible.onPressAction: choice.picked()
        Rectangle {
            anchors.fill: parent
            color: pickTap.pressed ? Theme.tint2 : choice.chosen ? Theme.tint1 : "transparent"
            EaseColor on color {}
        }
        Text {
            id: label
            anchors.left: parent.left
            anchors.leftMargin: 16
            anchors.right: keys.left
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: choice.text
            elide: Text.ElideRight
            color: Theme.ink1
            font: Theme.type.setting
        }
        TapHandler {
            id: pickTap
            onTapped: {
                Native.haptic(Native.Tick)
                choice.picked()
            }
        }
        Row {
            id: keys
            anchors.right: parent.right
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            IconKey {
                visible: choice.pick.length > 0
                anchors.verticalCenter: parent.verticalCenter
                iconName: "play"
                tone: "ghost"
                tip: qsTr("Preview %1").arg(choice.text)
                Accessible.name: tip
                onClicked: {
                    Native.haptic(Native.Tick)
                    core.previewPick(choice.pick)
                }
            }
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                width: Theme.iconLg
                opacity: choice.chosen ? 1 : 0
                name: "check"
                size: Theme.iconLg
                color: Theme.ink1
            }
        }
    }

    // A group's name in a sheet.
    component GroupLabel: Text {
        width: parent ? parent.width : 0
        leftPadding: 16
        rightPadding: 16
        topPadding: 12
        bottomPadding: 4
        elide: Text.ElideRight
        textFormat: Text.PlainText
        color: Theme.ink3
        font: Theme.type.captionStrong
        Accessible.role: Accessible.Heading
        Accessible.name: text
    }

    // A line under a group.
    component Note: Text {
        width: parent ? parent.width : 0
        leftPadding: 16
        rightPadding: 16
        topPadding: 4
        bottomPadding: 8
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        color: Theme.ink3
        font: Theme.type.description
    }
}
