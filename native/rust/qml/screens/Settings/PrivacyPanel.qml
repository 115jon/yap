pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// Privacy & safety (PrivacyPanel.tsx). Activity: "Share my activity",
// saved to the account. Direct messages: who can DM you, the three values
// as a segmented rail, saved to the account at once. Media filter: the four levels as a
// segmented rail, saved to the account at once (rolled back with the
// reason if it fails). Shared messages: the links from `/api/shares`, Live
// or All, each with Copy link, Open and Revoke (an inline confirm).
Column {
    id: root

    required property var host

    // The shared links, their filter, labels and revokes (shared with the
    // phone's Settings > Privacy & safety).
    readonly property SharesModel sharesModel: SharesModel {}
    readonly property var shares: sharesModel.shares
    readonly property var live: sharesModel.live

    spacing: 40
    Component.onCompleted: sharesModel.load()

    // Activity (PrivacyPanel.tsx ShareActivityRow): applied at once,
    // rolled back with the reason if the account does not take it.
    SettingsCard {
        width: parent.width
        sectionId: "activity"
        SwitchSetting {
            rowId: "share-activity"
            checked: Settings.shareActivity
            disabled: Settings.shareActivitySaving
            onToggled: Settings.shareMyActivity(!Settings.shareActivity)
            footer: RowError { text: Settings.shareActivityError }
        }
    }

    // Who can DM you (PrivacyPanel.tsx DmPermissionRow): the three values
    // as a segmented rail, saved at once (rolled back with the reason if
    // the account does not take it).
    SettingsCard {
        width: parent.width
        sectionId: "direct-messages"
        SettingsRow {
            rowId: "dm-permission"
            layout: "block"
            SegmentedField {
                width: parent.width
                accessibleName: SettingsNav.label("dm-permission")
                value: Settings.dmPermission
                busy: Settings.dmPermissionSaving.length > 0
                options: [
                    { value: "everyone", label: qsTr("Everyone"), desc: qsTr("Anyone on Yap can start a DM with you.") },
                    { value: "friends_and_servers", label: qsTr("Friends & servers"), desc: qsTr("Only your friends and people who share a server with you can start a DM.") },
                    { value: "nobody", label: qsTr("No one"), desc: qsTr("No one can start a new DM with you. Conversations you've written in stay open.") },
                ]
                onPicked: value => Settings.changeDmPermission(value)
            }
            footer: RowError { text: Settings.dmPermissionError }
        }
    }

    SettingsCard {
        width: parent.width
        sectionId: "media-filter"
        SettingsRow {
            rowId: "media-filter"
            layout: "block"
            SegmentedField {
                width: parent.width
                accessibleName: SettingsNav.label("media-filter")
                value: Settings.mediaFilter
                busy: Settings.mediaFilterSaving.length > 0
                options: [
                    { value: "high", label: qsTr("High"), badge: qsTr("G only"), desc: qsTr("Shows the strictest results for general audiences.") },
                    { value: "medium", label: qsTr("Medium"), badge: qsTr("G + PG"), desc: qsTr("Allows a little more expressiveness while staying broadly safe.") },
                    { value: "low", label: qsTr("Low"), badge: qsTr("Up to PG-13"), desc: qsTr("Includes more mature jokes and reactions without going fully open.") },
                    { value: "off", label: qsTr("Off"), badge: qsTr("Up to R"), desc: qsTr("Shows the widest provider-approved range. Explicit nudity is still excluded.") },
                ]
                onPicked: value => Settings.changeMediaFilter(value)
            }
            footer: RowError { text: Settings.mediaFilterError }
        }
    }

    SettingsCard {
        width: parent.width
        sectionId: "shared-messages"
        SettingsRow {
            rowId: "shared-messages"
            layout: "block"
            hideLabel: true
            Column {
                id: list
                width: parent.width
                spacing: 12

                Item {
                    width: parent.width
                    height: 36
                    Row {
                        id: shareTabs
                        height: parent.height
                        Accessible.role: Accessible.PageTabList
                        Accessible.name: qsTr("Shared links to show")
                        Tab { value: "current"; text: qsTr("Live"); count: root.live.length }
                        Tab { value: "all"; text: qsTr("All"); count: root.shares.length }
                    }
                    // The chosen tab's rule slides between tabs.
                    TabRule { row: shareTabs }
                    KitButton {
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        variant: "ghost"
                        size: "sm"
                        iconName: "refresh"
                        text: qsTr("Refresh")
                        loading: root.sharesModel.loading
                        onClicked: root.sharesModel.load()
                    }
                }

                readonly property var visibleShares: root.sharesModel.shown

                Row {
                    visible: root.sharesModel.loadingFirst
                    spacing: 8
                    Icon {
                        id: spinner
                        anchors.verticalCenter: parent.verticalCenter
                        name: "loader"
                        size: Theme.iconSm
                        color: Theme.ink2
                        smooth: true
                        RotationAnimator on rotation {
                            from: 0
                            to: 360
                            duration: Theme.durSpin
                            loops: Animation.Infinite
                            running: spinner.visible && !Theme.reducedMotion
                        }
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: qsTr("Loading shared messages…")
                        color: Theme.ink2
                        font: Theme.type.description
                    }
                }
                RowError {
                    above: 0
                    text: root.sharesModel.failed ? root.sharesModel.error : ""
                }
                Text {
                    visible: root.sharesModel.empty
                    width: parent.width
                    textFormat: Text.PlainText
                    text: root.sharesModel.emptyText
                    wrapMode: Text.Wrap
                    color: Theme.ink2
                    font: Theme.type.description
                }
                Column {
                    width: parent.width
                    visible: !root.sharesModel.failed
                    Repeater {
                        model: list.visibleShares
                        delegate: ShareItem {}
                    }
                }
            }
        }
    }

    // A tab with its count (ui/tabs.tsx): ink-1 and a 2 px rule when chosen.
    component Tab: T.AbstractButton {
        id: tab
        property string value
        property int count
        readonly property bool on: root.sharesModel.filter === value
        implicitWidth: implicitContentWidth + leftPadding + rightPadding
        height: 36
        leftPadding: 8
        rightPadding: 8
        focusPolicy: Qt.StrongFocus
        Accessible.role: Accessible.PageTab
        Accessible.name: text
        Accessible.selected: on
        onClicked: root.sharesModel.filter = value
        contentItem: Row {
            spacing: 8
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: tab.text
                color: tab.on || tab.hovered ? Theme.ink1 : Theme.ink2
                EaseColor on color {}
                font: Theme.type.controlSm
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.max(20, countText.implicitWidth + 12)
                height: 20
                radius: Theme.radius
                color: Theme.tint2
                Text {
                    id: countText
                    anchors.centerIn: parent
                    textFormat: Text.PlainText
                    text: tab.count
                    color: Theme.ink2
                    font: Theme.type.numSm
                }
            }
        }
        // The rule is the strip's TabRule.
        background: Item {
            Rectangle {
                anchors.fill: parent
                visible: tab.visualFocus
                color: "transparent"
                border.width: 2
                border.color: Theme.focus
            }
        }
        HoverHandler { cursorShape: Qt.PointingHandCursor }
    }

    // One link: who, what, its LED status with the dates and views, and
    // its keys while it is live.
    component ShareItem: Column {
        id: share
        required property var modelData
        required property int index
        readonly property bool active: modelData.status === "active"
        property bool confirming: false
        property bool copied: false

        function dismiss() {
            if (Settings.revokingShare.length === 0)
                cancelRevoke()
        }
        function cancelRevoke() {
            confirming = false
            root.host.popEscape(share)
        }
        Component.onDestruction: root.host.popEscape(share)
        Connections {
            target: Settings
            function onRevokingShareChanged() {
                if (Settings.revokingShare.length === 0 && share.confirming && Settings.shareErrorId !== share.modelData.id)
                    share.cancelRevoke()
            }
        }

        width: parent ? parent.width : 0
        bottomPadding: 12

        Rectangle {
            visible: share.index > 0
            width: parent.width
            height: 1
            color: Theme.line
        }
        Item {
            visible: share.index > 0
            width: 1
            height: 12
        }
        Item {
            width: parent.width
            height: Math.max(info.height, keys.height)
            Column {
                id: info
                width: parent.width - (keys.visible ? keys.width + 16 : 0)
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
                    topPadding: 2
                    textFormat: Text.PlainText
                    text: root.sharesModel.content(share.modelData)
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
                        text: root.sharesModel.statusLabel(share.modelData.status)
                        ledState: root.sharesModel.ledState(share.modelData.status)
                    }
                    Meta { text: root.sharesModel.sharedText(share.modelData) }
                    Meta { text: root.sharesModel.expiresText(share.modelData) }
                    Meta { text: root.sharesModel.viewsText(share.modelData) }
                }
            }
            Row {
                id: keys
                visible: share.active && !share.confirming
                anchors.right: parent.right
                spacing: 6
                KitButton {
                    size: "sm"
                    iconName: share.copied ? "check" : "copy"
                    text: share.copied ? qsTr("Copied") : qsTr("Copy link")
                    onClicked: {
                        clipboard.copyText(share.modelData.url)
                        share.copied = true
                        copiedTimer.restart()
                    }
                }
                KitButton {
                    size: "sm"
                    iconName: "externalLink"
                    text: qsTr("Open")
                    onClicked: Qt.openUrlExternally(share.modelData.url)
                }
                KitButton {
                    id: revokeKey
                    variant: "destructive"
                    size: "sm"
                    text: qsTr("Revoke")
                    onClicked: {
                        share.confirming = true
                        root.host.pushEscape(share)
                    }
                }
            }
            Timer {
                id: copiedTimer
                interval: 1800
                onTriggered: share.copied = false
            }
        }
        Loader {
            width: parent.width
            active: share.confirming
            visible: active
            sourceComponent: ConfirmStrip {
                consequence: qsTr("Anyone with this link will see that it's gone. You can't undo this.")
                confirmLabel: qsTr("Revoke link")
                irreversible: true
                busy: root.sharesModel.busy(share.modelData.id)
                error: root.sharesModel.errorFor(share.modelData.id)
                onCancelled: share.cancelRevoke()
                onConfirmed: root.sharesModel.revoke(share.modelData.id)
            }
        }
    }

    component Meta: Text {
        textFormat: Text.PlainText
        height: 20
        verticalAlignment: Text.AlignVCenter
        color: Theme.ink2
        font: Theme.type.caption
    }

    // Copies text without touching anything else (a hidden edit).
    TextEdit {
        id: clipboard
        visible: false
        function copyText(text) {
            clipboard.text = text
            clipboard.selectAll()
            clipboard.copy()
        }
    }
}
