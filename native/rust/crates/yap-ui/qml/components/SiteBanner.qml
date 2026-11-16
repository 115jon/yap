import QtQuick
import Yap.Ui

// The site announcement strip (SiteAnnouncementBar.tsx), wired to
// Session.announcement (GET /api/site-config, kept fresh by the core): an
// LED for the tone (unlit for info, idle amber for a warning, red when
// critical), the tone's word in silkscreen (the LED alone on a narrow
// window, as the web's `max-sm`), the text with its optional link
// inline, and a dismiss key a critical one does not get. Dismissal is per
// announcement and per device (Session.dismissAnnouncement). It hides
// once its end time passes, between the core's refreshes.
Rectangle {
    id: root

    // Clear of a phone's side cut-outs in landscape.
    property real leftInset: 0
    property real rightInset: 0

    // The announcement's link (http(s) only): each app opens it its own
    // way, outside the app as the web's target=_blank does.
    signal linkRequested(string url)

    readonly property var announcement: {
        if (Session.announcement.length === 0)
            return null
        try {
            return JSON.parse(Session.announcement)
        } catch (error) {
            return null
        }
    }
    readonly property bool expiring: announcement !== null && announcement.expiresAt.length > 0
    // Ticks only while there is an end time to watch.
    property double now: Date.now()
    readonly property bool shown: announcement !== null
        && (!expiring || Date.parse(announcement.expiresAt) > now)
    readonly property string tone: announcement ? announcement.tone : "info"
    readonly property bool critical: tone === "critical"
    readonly property string word: critical ? qsTr("Important")
        : tone === "warning" ? qsTr("Heads up") : qsTr("Notice")
    readonly property bool narrow: width < 640

    implicitHeight: shown ? Math.max(36, body.implicitHeight + 12) : 0
    visible: shown
    // The tints are translucent: an opaque ground under them, so the rows
    // that scroll behind the strip (the phone's chat) never show through.
    color: Theme.ply2
    clip: true

    Rectangle {
        anchors.fill: parent
        visible: root.critical || root.tone === "warning"
        color: root.critical ? Theme.dangerTint : Theme.warningTint
    }

    // Presses on the strip stay on it: on the phone the chat's rows scroll
    // behind it, and a tap here must not reach the row underneath.
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        preventStealing: true
    }

    Timer {
        // Only while it shows on screen with an end time to watch.
        running: root.expiring && root.visible && Qt.application.state === Qt.ApplicationActive
        repeat: true
        triggeredOnStart: true
        interval: 15000
        onTriggered: root.now = Date.now()
    }

    function escaped(text) {
        return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
            .replace(/"/g, "&quot;")
    }

    Row {
        id: label
        x: 12 + root.leftInset
        anchors.verticalCenter: parent.verticalCenter
        spacing: 8
        Led {
            anchors.verticalCenter: parent.verticalCenter
            lit: root.tone !== "info"
            color: root.critical ? Theme.dangerInk : Theme.idle
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            visible: !root.narrow
            textFormat: Text.PlainText
            text: root.word
            color: Theme.ink2
            font: Theme.type.silk
        }
    }

    Text {
        id: body
        anchors.left: label.right
        anchors.leftMargin: 12
        anchors.right: dismiss.visible ? dismiss.left : parent.right
        anchors.rightMargin: dismiss.visible ? 8 : 12 + root.rightInset
        anchors.verticalCenter: parent.verticalCenter
        wrapMode: Text.Wrap
        textFormat: Text.StyledText
        linkColor: Theme.accentInk
        color: Theme.ink1
        font: Theme.type.description
        text: {
            const a = root.announcement
            if (!a)
                return ""
            const words = root.escaped(a.text)
            // The href is a fixed token: StyledText reads attribute values
            // raw (no entities, cut at a quote), so the URL itself is
            // taken from the announcement when the link is activated.
            return a.linkUrl.length > 0
                ? words + " <a href=\"announcement\">" + root.escaped(a.linkLabel) + "</a>"
                : words
        }
        // Only http(s) links reach here (the core drops anything else).
        onLinkActivated: () => {
            const url = root.announcement ? root.announcement.linkUrl : ""
            if (/^https?:\/\//i.test(url))
                root.linkRequested(url)
        }
        HoverHandler {
            enabled: Theme.hoverUi
            cursorShape: body.hoveredLink.length > 0 ? Qt.PointingHandCursor : Qt.ArrowCursor
        }
    }

    IconKey {
        id: dismiss
        anchors.right: parent.right
        anchors.rightMargin: 8 + root.rightInset
        anchors.verticalCenter: parent.verticalCenter
        visible: !root.critical
        size: 24
        iconName: "close"
        tone: "muted"
        tip: qsTr("Dismiss")
        onClicked: Session.dismissAnnouncement()
    }

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.line
    }

    Accessible.role: root.critical ? Accessible.AlertMessage : Accessible.StaticText
    Accessible.name: root.announcement ? root.word + ". " + root.announcement.text : ""
}
