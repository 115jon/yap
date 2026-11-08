pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Yap.Chrome
import com.yap.native

// Device-code sign-in, the desktop app's PublicFrame (src/components/auth/
// PublicFrame.tsx, home = null; panels from DesktopLogin.tsx): a still
// dither panel on the leading edge (a short band across the top when
// narrow), the logo lockup, and a 440 px left-aligned column that grows
// downward from a fixed offset.
//
// The Session only exposes phase / userCode / verificationUrl / status, so
// the web's verifying, success and unreachable panels have no native twin
// (Loading hands over to the boot screen). The failure panels are chosen by
// the text of the status the sign-in left behind (yap-core auth.rs).
Rectangle {
    id: root

    // auth.rs AuthError::Expired / Denied and reduce.rs SESSION_ENDED, verbatim.
    readonly property string expiredText: "The code expired. Start again."
    readonly property string deniedText: "Sign-in was denied."
    readonly property string sessionEndedText: "Your session ended. Sign in again."

    readonly property bool wide: width >= 1024
    readonly property bool inFlow: Session.phase === Session.DeviceCode
    readonly property bool hasCode: Session.userCode.length > 0
    // A code has been asked for and has not come back yet.
    readonly property bool starting: inFlow && !hasCode

    // Why the last attempt ended (a status line), or "". Read a tick after
    // the phase moves, since the reducer sends phase before status. A
    // cancel is not a failure, and the status it leaves behind is stale.
    property string outcome: ""
    property bool cancelled: false

    // idle | pending | expired | denied | error
    readonly property string view: inFlow && hasCode ? "pending"
        : outcome === expiredText ? "expired"
        : outcome === deniedText ? "denied"
        : outcome.length > 0 && outcome !== sessionEndedText ? "error"
        : "idle"
    readonly property bool displayTitle: view === "idle"

    readonly property string tileIcon: view === "pending" ? "globe"
        : view === "expired" ? "timerOff"
        : view === "denied" ? "shieldX"
        : view === "error" ? "alert"
        : ""
    readonly property string tileTone: view === "pending" ? "accent"
        : view === "error" ? "danger"
        : "neutral"
    readonly property string titleText: view === "idle" ? qsTr("Sign in to Yap")
        : view === "pending" ? qsTr("Check your browser")
        : view === "expired" ? qsTr("That code expired")
        : view === "denied" ? qsTr("Sign-in declined")
        : qsTr("Couldn't sign in")
    readonly property string bodyText: view === "idle" ? qsTr("Your browser opens to confirm it's you, then you're back here.")
        : view === "pending" ? qsTr("Make sure it shows this same code, then confirm.")
        : view === "expired" ? qsTr("Codes only last a few minutes. Get a fresh one to keep going.")
        : view === "denied" ? qsTr("The request was declined in your browser, so nothing was signed in.")
        : qsTr("Check your internet connection, then try again.")

    // Display type (public-type.ts): Bricolage 800, 44 to 68 px fluid,
    // leading 0.9, tracking -0.04em.
    readonly property real displaySize: Math.round(Math.max(44, Math.min(68, 35.2 + 0.026 * width)))

    property bool linkCopied: false

    color: Theme.ply0

    function capture() {
        outcome = cancelled ? "" : Session.status
        cancelled = false
    }

    function startSignIn() {
        if (!starting)
            Session.startSignIn()
    }

    function copyLink() {
        // ClipboardText: YAP_NO_CLIPBOARD runs only log it.
        clip.copyText(Session.verificationUrl)
        linkCopied = true
        copiedTimer.restart()
    }

    Component.onCompleted: Qt.callLater(capture)

    // The browser opens by itself once per code (DesktopLogin's
    // openedUrlRef opens verification_uri_complete when pending). The URL
    // can land after the code, so this follows the URL, not the code.
    // Files.openExternal: YAP_LOG_EXTERNAL_OPEN test runs only log it.
    readonly property string pendingUrl: inFlow && hasCode ? Session.verificationUrl : ""
    property string openedUrl: ""
    onHasCodeChanged: if (hasCode && inFlow) outcome = ""
    onPendingUrlChanged: {
        if (pendingUrl.length > 0 && pendingUrl !== openedUrl) {
            openedUrl = pendingUrl
            Files.openExternal(pendingUrl)
        }
    }

    onViewChanged: enter.restart()

    Connections {
        target: Session
        function onPhaseChanged() {
            if (Session.phase === Session.SignedOut)
                Qt.callLater(root.capture)
            else if (Session.phase === Session.DeviceCode)
                root.cancelled = false
        }
    }

    Timer {
        id: copiedTimer
        interval: 2000
        onTriggered: root.linkCopied = false
    }

    // The app's clipboard helper (YAP_NO_CLIPBOARD logs instead).
    ClipboardText { id: clip }

    // ── The dither: one painted frame, decorative ───────────────────────
    readonly property int artWidth: Math.min(600, Math.round(width * 0.36))
    readonly property int bandHeight: 64

    Rectangle {
        id: art
        readonly property int cell: root.wide ? 7 : 4
        readonly property int cols: Math.max(1, Math.ceil(width / cell))
        readonly property int rows: Math.max(1, Math.ceil(height / cell))

        width: root.wide ? root.artWidth : root.width
        height: root.wide ? root.height : root.bandHeight
        color: Theme.ply1
        clip: true
        Accessible.ignored: true

        Image {
            width: art.cols * art.cell
            height: art.rows * art.cell
            smooth: false
            cache: false
            fillMode: Image.Stretch
            source: art.width > 0 && art.height > 0
                ? "image://dither/ramp/" + (root.wide ? "72/14/45/" : "60/14/30/") + Theme.hex(Theme.accentInk) + "/" + art.cols + "/" + art.rows
                : ""
            Accessible.ignored: true
        }
        Rectangle {
            anchors.right: root.wide ? parent.right : undefined
            anchors.bottom: root.wide ? undefined : parent.bottom
            width: root.wide ? 1 : parent.width
            height: root.wide ? parent.height : 1
            color: Theme.line
        }
    }

    // ── Header and column ───────────────────────────────────────────────
    Item {
        id: frame
        x: root.wide ? root.artWidth : 0
        y: root.wide ? 0 : root.bandHeight
        width: root.width - x
        height: root.height - y

        Item {
            id: header
            x: 24
            y: 16
            height: 44
            width: lockup.implicitWidth

            RowLayout {
                id: lockup
                anchors.fill: parent
                spacing: 12
                Accessible.role: Accessible.StaticText
                Accessible.name: qsTr("Yap")

                Item {
                    Layout.preferredWidth: 28
                    Layout.preferredHeight: 28
                    Accessible.ignored: true
                    YapBark {
                        anchors.centerIn: parent
                        cell: 1
                    }
                }
                Text {
                    textFormat: Text.PlainText
                    text: qsTr("Yap")
                    color: Theme.ink1
                    font: Theme.type.serverTitle
                }
            }
        }

        Flickable {
            id: flick
            anchors.top: header.bottom
            anchors.bottom: parent.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            contentWidth: width
            contentHeight: column.y + column.implicitHeight + 48
            ScrollBar.vertical: ThinScrollBar {}

            ColumnLayout {
                id: column

                // lift drives the 4 px rise of a new panel.
                property real lift: 0
                readonly property real topOffset: root.width >= 768
                    ? Math.max(32, Math.min(96, Math.round(root.height * 0.10)))
                    : 40

                x: Math.round((flick.width - width) / 2)
                y: topOffset + lift
                width: Math.min(440, flick.width - 48)
                spacing: 0

                ParallelAnimation {
                    id: enter
                    NumberAnimation { target: column; property: "opacity"; from: 0; to: 1; duration: Theme.dur3 }
                    NumberAnimation {
                        target: column
                        property: "lift"
                        from: Theme.reducedMotion ? 0 : 4
                        to: 0
                        duration: Theme.dur3
                        easing.type: Easing.BezierSpline
                        easing.bezierCurve: Theme.easeOut
                    }
                }

                // The status tile: 48 px, tinted by tone, a 24 px glyph.
                StatusTile {
                    Layout.bottomMargin: 24
                    visible: !root.displayTitle
                    icon: root.tileIcon
                    tone: root.tileTone
                }

                Text {
                    Layout.fillWidth: true
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                    text: root.titleText
                    color: Theme.ink1
                    font: root.displayTitle
                        ? Qt.font({
                              family: Theme.type.headline.family,
                              pixelSize: root.displaySize,
                              weight: Font.ExtraBold,
                              letterSpacing: -0.04 * root.displaySize,
                              hintingPreference: Theme.type.headline.hintingPreference
                          })
                        : Theme.type.dialogTitle
                    lineHeight: root.displayTitle ? 0.9 : Theme.type.dialogTitleLine
                    lineHeightMode: root.displayTitle ? Text.ProportionalHeight : Text.FixedHeight
                    Accessible.role: Accessible.Heading
                }

                // The poster's lip: full column under the display title,
                // 240 px max under the smaller ones.
                DitherRule {
                    Layout.topMargin: root.displayTitle ? 24 : 16
                    Layout.preferredWidth: root.displayTitle ? column.width : Math.min(240, column.width)
                }

                Text {
                    Layout.topMargin: root.displayTitle ? 24 : 16
                    Layout.fillWidth: true
                    Layout.maximumWidth: root.displayTitle ? 380 : 400
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                    text: root.bodyText
                    color: Theme.ink2
                    font: root.displayTitle
                        ? Qt.font({
                              family: Theme.type.message.family,
                              pixelSize: 17,
                              weight: Font.Normal,
                              hintingPreference: Theme.type.message.hintingPreference
                          })
                        : Theme.type.message
                    lineHeight: root.displayTitle ? 24 : Theme.type.messageLine
                    lineHeightMode: Text.FixedHeight
                }

                // The device code, matched by eye against the browser's.
                Rectangle {
                    Layout.topMargin: 24
                    Layout.fillWidth: true
                    Layout.preferredHeight: 64
                    visible: root.view === "pending"
                    radius: Theme.radius
                    color: Theme.ply2
                    border.width: 1
                    border.color: Theme.lineStrong
                    Accessible.role: Accessible.StaticText
                    Accessible.name: qsTr("Your code is %1").arg(Session.userCode.split("").join(" "))

                    Row {
                        anchors.centerIn: parent
                        spacing: 12
                        Repeater {
                            model: Session.userCode.split("-")
                            delegate: Row {
                                id: group
                                required property string modelData
                                required property int index
                                spacing: 12
                                Rectangle {
                                    visible: group.index > 0
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: 12
                                    height: 2
                                    color: Theme.ink3
                                }
                                Text {
                                    textFormat: Text.PlainText
                                    text: group.modelData
                                    color: Theme.ink1
                                    font: Theme.type.deviceCode
                                }
                            }
                        }
                    }
                }

                LedLabel {
                    Layout.topMargin: 16
                    visible: root.view === "pending"
                    ledState: "idle"
                    text: qsTr("Waiting for you to confirm")
                }

                // Idle and failure panels: one full-width 48 px key.
                KitButton {
                    Layout.topMargin: 32
                    Layout.fillWidth: true
                    visible: root.view !== "pending"
                    variant: "primary"
                    size: "lg"
                    iconName: root.view === "idle" ? "externalLink" : ""
                    loading: root.starting
                    text: root.starting
                        ? (root.view === "idle" ? qsTr("Getting your code…") : qsTr("Getting a new code…"))
                        : root.view === "idle" ? qsTr("Sign in with browser") : qsTr("Try again")
                    onClicked: root.startSignIn()
                }

                // Pending: open again and copy the link side by side, then cancel.
                RowLayout {
                    Layout.topMargin: 32
                    Layout.fillWidth: true
                    visible: root.view === "pending"
                    spacing: 8
                    KitButton {
                        Layout.fillWidth: true
                        Layout.preferredWidth: 1
                        variant: "secondary"
                        size: "lg"
                        iconName: "externalLink"
                        text: qsTr("Open browser again")
                        onClicked: Files.openExternal(Session.verificationUrl)
                    }
                    KitButton {
                        Layout.fillWidth: true
                        Layout.preferredWidth: 1
                        variant: "secondary"
                        size: "lg"
                        iconName: root.linkCopied ? "check" : "link"
                        text: root.linkCopied ? qsTr("Link copied") : qsTr("Copy link")
                        onClicked: root.copyLink()
                    }
                }
                KitButton {
                    Layout.topMargin: 8
                    Layout.fillWidth: true
                    visible: root.view === "pending"
                    variant: "ghost"
                    size: "lg"
                    text: qsTr("Cancel")
                    onClicked: {
                        root.cancelled = true
                        Session.cancelSignIn()
                    }
                }

                // Why it stopped: the status line itself, under the key (a session
                // that ended on its own keeps the idle panel and gets only this).
                Notice {
                    Layout.topMargin: 16
                    Layout.fillWidth: true
                    visible: root.view === "error" || (root.view === "idle" && root.outcome === root.sessionEndedText)
                    icon: root.view === "error" ? "alert" : "info"
                    danger: root.view === "error"
                    text: root.outcome
                }

                // Dev: paste a session token instead. Quiet on purpose.
                Text {
                    Layout.topMargin: 40
                    visible: !root.inFlow
                    textFormat: Text.PlainText
                    text: tokenRow.visible ? qsTr("Hide token sign-in") : qsTr("Developer: use a session token")
                    color: devHover.hovered ? Theme.ink2 : Theme.ink3
                    EaseColor on color {}
                    font: Theme.type.caption
                    HoverHandler { id: devHover; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: tokenRow.visible = !tokenRow.visible }
                }
                RowLayout {
                    id: tokenRow
                    Layout.topMargin: 8
                    Layout.fillWidth: true
                    visible: false
                    spacing: 8
                    TextField {
                        // No Qt 6.9+ default edit menu (the web has none of its own).
                        ContextMenu.menu: null
                        id: tokenField
                        Layout.fillWidth: true
                        implicitHeight: 40
                        echoMode: TextInput.Password
                        placeholderText: "Session token"
                        placeholderTextColor: Theme.ink3
                        color: Theme.ink1
                        selectionColor: Theme.selection
                        font: Theme.type.code
                        leftPadding: 12
                        background: Rectangle {
                            radius: Theme.radius
                            color: Theme.ply2
                            border.width: 1
                            border.color: tokenField.activeFocus ? Theme.ink1 : Theme.lineField
                        }
                        onAccepted: useButton.clicked()
                    }
                    KitButton {
                        id: useButton
                        implicitHeight: 40
                        variant: "secondary"
                        text: "Use"
                        enabled: tokenField.text.length > 0
                        onClicked: {
                            Session.useToken(tokenField.text)
                            tokenField.clear()
                        }
                    }
                }
            }
        }
    }

    // ── Parts ───────────────────────────────────────────────────────────

    // PublicFrame StatusTile: 48 px square, tinted by tone, a 24 px glyph.
    component StatusTile: Rectangle {
        id: tile

        property string icon
        property string tone: "neutral"
        readonly property color ink: tone === "accent" ? Theme.accentInk
            : tone === "danger" ? Theme.dangerInk
            : tone === "success" ? Theme.successInk
            : Theme.ink2

        implicitWidth: 48
        implicitHeight: 48
        radius: Theme.radius
        color: tone === "accent" ? Theme.accentTint
            : tone === "danger" ? Theme.dangerTint
            : tone === "success" ? Theme.successTint
            : Theme.tint1
        Accessible.ignored: true

        Icon {
            anchors.centerIn: parent
            name: tile.icon
            size: Theme.iconXl
            color: tile.ink
        }
    }

    // PublicFrame Notice: an inline message strip, 16 px glyph, 2 px corners.
    component Notice: Rectangle {
        id: notice

        property string text
        property string icon: "info"
        property bool danger: false

        implicitHeight: noticeRow.implicitHeight + 24
        radius: Theme.radius
        color: danger ? Theme.dangerTint : Theme.tint1
        Accessible.role: Accessible.StaticText
        Accessible.name: text

        RowLayout {
            id: noticeRow
            anchors.fill: parent
            anchors.margins: 12
            spacing: 8
            Icon {
                Layout.alignment: Qt.AlignTop
                Layout.topMargin: 1
                name: notice.icon
                size: Theme.iconSm
                color: notice.danger ? Theme.dangerInk : Theme.ink3
            }
            Text {
                Layout.fillWidth: true
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                text: notice.text
                color: notice.danger ? Theme.dangerInk : Theme.ink2
                font: Theme.type.description
            }
        }
    }
}
