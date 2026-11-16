pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Settings > Privacy & safety (the desktop's PrivacyPanel): Share my
// activity, who can DM you and the media filter, each saved to the account
// at once and rolled back with the reason if it does not take them; then
// Shared messages: the links you've shared, Live or All, each with its
// status, dates and views. A tap or a long press on a live link opens Copy
// link, Open and Revoke link (confirmed in a sheet).
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

    // The shared message links (the desktop's list, one SharesModel).
    readonly property SharesModel shares: SharesModel {}
    Component.onCompleted: shares.load()

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

    SettingsSection {
        number: 4
        title: qsTr("Shared messages")
        Label {
            title: qsTr("Your shared links")
            body: qsTr("Copy, open or revoke links you've shared. Tap a link, or press and hold it, for its actions.")
        }
        Segmented {
            width: parent.width
            accessibleName: qsTr("Shared links to show")
            options: [qsTr("Live (%1)").arg(root.shares.live.length), qsTr("All (%1)").arg(root.shares.shares.length)]
            current: root.shares.filter === "current" ? 0 : 1
            onPicked: index => root.shares.filter = index === 0 ? "current" : "all"
        }
        Item { width: 1; height: 8 }
        Caption {
            visible: root.shares.loadingFirst
            text: qsTr("Loading shared messages…")
        }
        Column {
            visible: root.shares.failed
            width: parent.width
            spacing: 8
            SettingsErrorText { text: root.shares.error }
            KitButton {
                variant: "secondary"
                text: qsTr("Try again")
                onClicked: root.shares.load()
            }
        }
        Caption {
            visible: root.shares.empty
            text: root.shares.emptyText
        }
        Column {
            visible: !root.shares.failed
            width: parent.width
            Repeater {
                model: root.shares.shown
                delegate: ShareRow {}
            }
        }
    }

    // The link the actions sheet and the revoke confirm act on.
    property var picked: null
    // The link this page last asked to revoke: only its failure shows.
    property string revokedHere: ""
    function openActions(share) {
        root.picked = share
        actions.title = share.author
        actions.open()
    }

    overlays: [
        BottomSheet {
            id: actions
            anchors.fill: parent
            contentHeight: actionColumn.implicitHeight

            Column {
                id: actionColumn
                width: parent.width
                bottomPadding: 8
                Text {
                    x: 16
                    width: parent.width - 32
                    bottomPadding: 8
                    textFormat: Text.PlainText
                    text: root.picked ? root.shares.content(root.picked) : ""
                    wrapMode: Text.Wrap
                    maximumLineCount: 3
                    elide: Text.ElideRight
                    color: Theme.ink2
                    font: Theme.type.description
                }
                SheetRow {
                    iconName: "copy"
                    text: qsTr("Copy link")
                    onTriggered: {
                        Native.haptic(Native.Tick)
                        // Android shows its own "Copied" note.
                        Native.copy(root.picked.url)
                        actions.close()
                    }
                }
                SheetRow {
                    iconName: "externalLink"
                    text: qsTr("Open")
                    onTriggered: {
                        Qt.openUrlExternally(root.picked.url)
                        actions.close()
                    }
                }
                SheetRow {
                    // One revoke at a time (the core drops a second).
                    visible: root.shares.revoking.length === 0
                    iconName: "trash"
                    danger: true
                    text: qsTr("Revoke link")
                    onTriggered: {
                        actions.close()
                        revokeSheet.open()
                    }
                }
            }
        },
        ConfirmSheet {
            id: revokeSheet
            anchors.fill: parent
            title: qsTr("Revoke this link?")
            body: qsTr("Anyone with this link will see that it's gone. You can't undo this.")
            quote: root.picked ? root.shares.content(root.picked) : ""
            confirmText: qsTr("Revoke link")
            danger: true
            onConfirmed: {
                if (root.shares.revoking.length > 0)
                    return
                Native.haptic(Native.Confirm)
                root.revokedHere = root.picked.id
                root.shares.revoke(root.picked.id)
            }
        }
    ]

    // One link: who and what, its LED status with the dates and views; a
    // tap or a long press opens its actions.
    component ShareRow: Item {
        id: share
        required property var modelData
        required property int index
        readonly property bool busy: root.shares.busy(modelData.id)
        readonly property string error: root.revokedHere === modelData.id ? root.shares.errorFor(modelData.id) : ""

        width: parent ? parent.width : 0
        height: info.implicitHeight + 24
        readonly property bool active: modelData.status === "active"
        Accessible.role: active ? Accessible.Button : Accessible.StaticText
        Accessible.name: qsTr("%1: %2").arg(modelData.author).arg(root.shares.content(modelData))
        Accessible.description: [root.shares.statusLabel(modelData.status), root.shares.sharedText(modelData),
            root.shares.expiresText(modelData), root.shares.viewsText(modelData)].join(", ")
        Accessible.onPressAction: if (share.active) root.openActions(share.modelData)

        Rectangle {
            visible: share.index > 0
            width: parent.width
            height: 1
            color: Theme.line
        }
        Rectangle {
            anchors.fill: parent
            anchors.leftMargin: -8
            anchors.rightMargin: -8
            radius: Theme.radius
            color: tap.pressed && share.active ? Theme.tint2 : "transparent"
            Behavior on color { ColorAnimation { duration: Theme.dur1 } }
        }
        Column {
            id: info
            y: 12
            width: parent.width - (more.visible ? more.width + 8 : 0)
            spacing: 2
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: share.modelData.author
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.setting
            }
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: root.shares.content(share.modelData)
                wrapMode: Text.Wrap
                maximumLineCount: 2
                elide: Text.ElideRight
                color: Theme.ink2
                font: Theme.type.description
            }
            Flow {
                width: parent.width
                topPadding: 4
                spacing: 12
                LedLabel {
                    text: root.shares.statusLabel(share.modelData.status)
                    ledState: root.shares.ledState(share.modelData.status)
                }
                Meta { text: root.shares.sharedText(share.modelData) }
                Meta { text: root.shares.expiresText(share.modelData) }
                Meta { text: root.shares.viewsText(share.modelData) }
            }
            Text {
                visible: share.busy
                width: parent.width
                textFormat: Text.PlainText
                text: qsTr("Revoking…")
                color: Theme.ink3
                font: Theme.type.caption
            }
            SettingsErrorText { text: share.error }
        }
        Icon {
            id: more
            visible: share.active
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            name: "more"
            size: Theme.iconMd
            color: Theme.ink3
        }
        TapHandler {
            id: tap
            enabled: share.active
            onTapped: {
                Native.haptic(Native.Tick)
                root.openActions(share.modelData)
            }
            onLongPressed: {
                Native.haptic(Native.LongPress)
                root.openActions(share.modelData)
            }
        }
    }

    component Meta: Text {
        textFormat: Text.PlainText
        height: 20
        verticalAlignment: Text.AlignVCenter
        color: Theme.ink3
        font: Theme.type.caption
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
