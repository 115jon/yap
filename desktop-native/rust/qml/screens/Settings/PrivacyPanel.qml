pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import com.yap.native

// Privacy & safety (PrivacyPanel.tsx). Media filter: the four levels as a
// segmented rail, saved to the account at once (rolled back with the
// reason if it fails). Shared messages: the links from `/api/shares`, Live
// or All, each with Copy link, Open and Revoke (an inline confirm).
Column {
    id: root

    required property var host

    readonly property var shares: JSON.parse(Settings.shares)
    readonly property var live: shares.filter(share => share.status === "active")
    property string filter: "current"

    spacing: 40
    Component.onCompleted: Settings.loadShares()

    function formatDate(iso) {
        if (!iso)
            return qsTr("Never")
        const date = new Date(iso)
        return isNaN(date) ? qsTr("Never") : date.toLocaleDateString(Qt.locale(), "MMM d, yyyy")
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
                        height: parent.height
                        Accessible.role: Accessible.PageTabList
                        Accessible.name: qsTr("Shared links to show")
                        Tab { value: "current"; text: qsTr("Live"); count: root.live.length }
                        Tab { value: "all"; text: qsTr("All"); count: root.shares.length }
                    }
                    KitButton {
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        variant: "ghost"
                        size: "sm"
                        iconName: "refresh"
                        text: qsTr("Refresh")
                        loading: Settings.sharesState === "loading"
                        onClicked: Settings.loadShares()
                    }
                }

                readonly property var visibleShares: root.filter === "current" ? root.live : root.shares

                Row {
                    visible: Settings.sharesState === "loading" && root.shares.length === 0
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
                            duration: 900
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
                    text: Settings.sharesState === "error" ? Settings.sharesError : ""
                }
                Text {
                    visible: Settings.sharesState === "ready" && list.visibleShares.length === 0
                    width: parent.width
                    textFormat: Text.PlainText
                    text: root.filter === "current" ? qsTr("No live links. Share a message from its menu to make one.")
                        : qsTr("You haven't shared any messages yet.")
                    wrapMode: Text.Wrap
                    color: Theme.ink2
                    font: Theme.type.description
                }
                Column {
                    width: parent.width
                    visible: Settings.sharesState !== "error"
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
        readonly property bool on: root.filter === value
        implicitWidth: implicitContentWidth + leftPadding + rightPadding
        height: 36
        leftPadding: 8
        rightPadding: 8
        focusPolicy: Qt.StrongFocus
        Accessible.role: Accessible.PageTab
        Accessible.name: text
        Accessible.selected: on
        onClicked: root.filter = value
        contentItem: Row {
            spacing: 8
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: tab.text
                color: tab.on || tab.hovered ? Theme.ink1 : Theme.ink2
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
        background: Item {
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 2
                color: tab.on ? Theme.ink1 : "transparent"
            }
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
                    text: share.modelData.content.length > 0 ? share.modelData.content : qsTr("(Attachment only)")
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
                        text: share.modelData.status === "active" ? qsTr("Live")
                            : share.modelData.status === "revoked" ? qsTr("Revoked")
                            : share.modelData.status === "deleted" ? qsTr("Deleted") : qsTr("Expired")
                        ledState: share.modelData.status === "active" ? "live" : share.modelData.status === "expired" ? "idle" : "off"
                    }
                    Meta { text: qsTr("Shared %1").arg(root.formatDate(share.modelData.createdAt)) }
                    Meta { text: qsTr("Expires %1").arg(root.formatDate(share.modelData.expiresAt)) }
                    Meta { text: share.modelData.views === 1 ? qsTr("1 view") : qsTr("%1 views").arg(share.modelData.views) }
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
                busy: Settings.revokingShare === share.modelData.id
                error: Settings.shareErrorId === share.modelData.id ? Settings.shareError : ""
                onCancelled: share.cancelRevoke()
                onConfirmed: Settings.revokeShare(share.modelData.id)
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
