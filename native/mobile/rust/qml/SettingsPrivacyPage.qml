pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Settings > Privacy & safety (the desktop's PrivacyPanel): Share my
// activity, who can DM you and the media filter, each saved to the account
// at once and rolled back with the reason if it does not take them.
SettingsFrame {
    id: root

    // Who can DM you (the core's dm_permission): the value being saved,
    // else the account's.
    readonly property var dmLevels: [
        { value: "everyone", label: qsTr("Everyone"), desc: qsTr("Anyone on Yap can start a DM with you.") },
        { value: "friends_and_servers", label: qsTr("Friends & servers"), desc: qsTr("Only your friends and people who share a server with you can start a DM.") },
        { value: "nobody", label: qsTr("No one"), desc: qsTr("No one can start a new DM with you. Conversations you've written in stay open.") }
    ]
    readonly property string dmPermission: Settings.dmPermissionSaving.length > 0 ? Settings.dmPermissionSaving
        : Settings.dmPermission.length > 0 ? Settings.dmPermission : "everyone"

    // The media filter's levels: the one being saved, else the account's.
    readonly property var mediaLevels: [
        { value: "high", label: qsTr("High"), badge: qsTr("G only"), desc: qsTr("Shows the strictest results for general audiences.") },
        { value: "medium", label: qsTr("Medium"), badge: qsTr("G + PG"), desc: qsTr("Allows a little more expressiveness while staying broadly safe.") },
        { value: "low", label: qsTr("Low"), badge: qsTr("Up to PG-13"), desc: qsTr("Includes more mature jokes and reactions without going fully open.") },
        { value: "off", label: qsTr("Off"), badge: qsTr("Up to R"), desc: qsTr("Shows the widest provider-approved range. Explicit nudity is still excluded.") }
    ]
    readonly property string mediaFilter: Settings.mediaFilterSaving.length > 0 ? Settings.mediaFilterSaving
        : Settings.mediaFilter.length > 0 ? Settings.mediaFilter : "high"

    title: qsTr("Privacy & safety")

    SettingsSection {
        number: 1
        title: qsTr("Activity")
        SettingsSwitchRow {
            text: qsTr("Share my activity")
            description: qsTr("Show what you're playing to people who share a server, a DM or a friendship with you. Invisible always hides it.")
            checked: Settings.shareActivity
            locked: Settings.shareActivitySaving
            onToggled: Settings.shareMyActivity(!Settings.shareActivity)
        }
        SettingsErrorText { text: Settings.shareActivityError }
    }

    SettingsSection {
        number: 2
        title: qsTr("Direct messages")
        Label {
            title: qsTr("Who can DM you")
            body: qsTr("Applies to new messages and calls from people you haven't talked with. Blocking someone always closes a DM.")
        }
        Segmented {
            width: parent.width
            accessibleName: qsTr("Who can DM you")
            options: root.dmLevels.map(level => level.label)
            current: Math.max(0, root.dmLevels.findIndex(level => level.value === root.dmPermission))
            onPicked: index => Settings.changeDmPermission(root.dmLevels[index].value)
        }
        Item { width: 1; height: 8 }
        Caption {
            readonly property var level: root.dmLevels.find(l => l.value === root.dmPermission) ?? root.dmLevels[0]
            text: level.desc + (Settings.dmPermissionSaving.length > 0 ? " " + qsTr("Saving…") : "")
        }
        SettingsErrorText { text: Settings.dmPermissionError }
    }

    SettingsSection {
        number: 3
        title: qsTr("Media filter")
        Label {
            title: qsTr("Filter level")
            body: qsTr("Applies to GIF, sticker, clip and meme search and trending. Where a provider supports it, Yap sends your account ID so results match your choice.")
        }
        Segmented {
            width: parent.width
            accessibleName: qsTr("Media filter")
            options: root.mediaLevels.map(level => level.label)
            current: Math.max(0, root.mediaLevels.findIndex(level => level.value === root.mediaFilter))
            onPicked: index => Settings.changeMediaFilter(root.mediaLevels[index].value)
        }
        Item { width: 1; height: 8 }
        Caption {
            readonly property var level: root.mediaLevels.find(l => l.value === root.mediaFilter) ?? root.mediaLevels[0]
            text: level.badge + " · " + level.desc
                + (root.mediaFilter === "high" ? " " + qsTr("Media marked sensitive stays hidden until you show it.") : "")
        }
        SettingsErrorText { text: Settings.mediaFilterError }
    }

    // A setting's name over what it does, above its control.
    component Label: Column {
        id: label
        property string title
        property string body
        width: parent ? parent.width : 0
        bottomPadding: 12
        spacing: 2
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: label.title
            color: Theme.ink1
            font: Theme.type.setting
        }
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: label.body
            wrapMode: Text.Wrap
            color: Theme.ink3
            font: Theme.type.caption
        }
    }

    // What the chosen level means, read out as it changes.
    component Caption: Text {
        width: parent ? parent.width : 0
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: Theme.ink3
        font: Theme.type.caption
        Accessible.role: Accessible.StaticText
        Accessible.name: text
    }
}
