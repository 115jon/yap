import QtQuick
import Yap.Ui

// The account's shared message links, for the desktop's PrivacyPanel and
// the phone's Settings > Privacy & safety: `/api/shares` through the core
// (`Settings.shares`, each with its status already against the clock), the
// Live / All filter with each one's count, the load states, a link's status
// label and LED, its dates and views as text, and Revoke's busy and error
// (`Settings.revokingShare`, `shareErrorId`). Each surface keeps its own
// rows, keys and confirm.
QtObject {
    id: model

    readonly property var shares: {
        try {
            return JSON.parse(Settings.shares)
        } catch (e) {
            return []
        }
    }
    readonly property var live: shares.filter(share => share.status === "active")
    // `current` (Live) or `all`.
    property string filter: "current"
    readonly property var shown: filter === "current" ? live : shares

    readonly property bool loading: Settings.sharesState === "loading"
    // Nothing to show yet: the first load.
    readonly property bool loadingFirst: loading && shares.length === 0
    readonly property bool failed: Settings.sharesState === "error"
    readonly property string error: Settings.sharesError
    readonly property bool ready: Settings.sharesState === "ready"
    readonly property bool empty: ready && shown.length === 0
    readonly property string emptyText: filter === "current"
        ? qsTr("No live links. Share a message from its menu to make one.")
        : qsTr("You haven't shared any messages yet.")

    function load() {
        Settings.loadShares()
    }
    function revoke(id) {
        Settings.revokeShare(id)
    }
    // A revoke on its way (one at a time), and the one that failed.
    readonly property string revoking: Settings.revokingShare
    function busy(id) {
        return Settings.revokingShare === id
    }
    function errorFor(id) {
        return Settings.shareErrorId === id ? Settings.shareError : ""
    }

    function statusLabel(status) {
        return status === "active" ? qsTr("Live")
            : status === "revoked" ? qsTr("Revoked")
            : status === "deleted" ? qsTr("Deleted") : qsTr("Expired")
    }
    // LedLabel's state for a status.
    function ledState(status) {
        return status === "active" ? "live" : status === "expired" ? "idle" : "off"
    }
    function formatDate(iso) {
        if (!iso)
            return qsTr("Never")
        const date = new Date(iso)
        return isNaN(date) ? qsTr("Never") : date.toLocaleDateString(Qt.locale(), "MMM d, yyyy")
    }
    function content(share) {
        return share.content.length > 0 ? share.content : qsTr("(Attachment only)")
    }
    function sharedText(share) {
        return qsTr("Shared %1").arg(formatDate(share.createdAt))
    }
    function expiresText(share) {
        return qsTr("Expires %1").arg(formatDate(share.expiresAt))
    }
    function viewsText(share) {
        return share.views === 1 ? qsTr("1 view") : qsTr("%1 views").arg(share.views)
    }
}
