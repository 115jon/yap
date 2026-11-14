pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import Yap.Chrome
import com.yap.native
import Yap.Ui

// The invite page (routes/invite.$code.tsx, chat/InviteClient.tsx) as the
// web shows it: a whole public page over the app (below the title bar), on
// the one public frame (auth/PublicFrame.tsx, home = null as in the desktop
// app): the still dither panel on the leading edge from 1024 wide (a 64 px
// band across the top below that), the logo lockup, and a 440 px column
// top-aligned at clamp(32px, 10vh, 96px) that grows downward.
//   loading   the loaded invite's geometry as still placeholders
//   preview   the server's tile, "Invite · <inviter>", the server name as
//             the display title over the dithered rule, its member count,
//             then Accept invite and No thanks (48 px, full width)
//   joining   the same, Accept invite busy and No thanks off
//   joined    PageState: "Joined <server>", "Opening the server…"; it opens
//             after 1.5 s or on Open now
//   member    "You're already a member", "Open <server>"
//   invalid   "This invite isn't valid" (expired or missing), Open Yap
//   error     "Couldn't join the server" / "Couldn't load this invite",
//             Try again and Open Yap
// No thanks and Open Yap leave the page (the web goes to /chat); Escape
// does too unless a join is in flight. The web's signed-out states have no
// place here: it only opens signed in.
T.Popup {
    id: page

    // The core's landing, its state and copy (shared with the phone).
    readonly property InviteLandingState invite: InviteLandingState {}
    readonly property var landing: invite.landing
    readonly property string step: invite.step
    readonly property bool joining: invite.joining
    readonly property bool showsInvite: invite.showsInvite
    readonly property string serverName: invite.serverName

    readonly property bool wide: width >= 1024
    // public-type.ts PUBLIC_DISPLAY: clamp(2.75rem, 2.2rem + 2.6vw, 4.25rem).
    readonly property real displaySize: Math.round(Math.max(44, Math.min(68, 35.2 + 0.026 * width)))

    readonly property string tileIcon: invite.tileIcon
    readonly property string tileTone: invite.tileTone
    readonly property string titleText: invite.titleText
    readonly property string bodyText: invite.bodyText

    // The whole window under the title bar (none in stage fullscreen).
    parent: T.Overlay.overlay
    x: 0
    y: ShellState.stageFullscreen ? 0 : Theme.chrome
    width: parent ? parent.width : 0
    height: parent ? parent.height - y : 0
    padding: 0
    // Not modal: the title bar above stays live; the page itself takes
    // every press over the app.
    modal: false
    dim: false
    focus: true
    closePolicy: joining ? T.Popup.NoAutoClose : T.Popup.CloseOnEscape
    // Ctrl+K stays shut while it is up (ExtrasHost).
    onVisibleChanged: ShellState.modalCount += visible ? 1 : -1
    onClosed: Invites.closeLanding()
    onLandingChanged: if (!landing && opened) close()
    onStepChanged: enterAnim.restart()

    enter: PresenceIn { fromScale: 1 }
    exit: PresenceOut { toScale: 1 }

    background: Rectangle {
        color: Theme.ply0
        // Nothing under the page takes a press, a hover or a wheel.
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.AllButtons
            onWheel: wheel => wheel.accepted = true
        }
    }

    contentItem: Item {
        Accessible.role: Accessible.Pane
        Accessible.name: qsTr("Join Server")
        // What changed (InviteClient `announcement`).
        Accessible.description: page.invite.announcement

        // ── The dither: one painted frame, decorative ───────────────────
        readonly property int artWidth: Math.min(600, Math.round(width * 0.36))
        readonly property int bandHeight: 64

        Rectangle {
            id: art
            readonly property int cell: page.wide ? 7 : 4
            readonly property int cols: Math.max(1, Math.ceil(width / cell))
            readonly property int rows: Math.max(1, Math.ceil(height / cell))

            width: page.wide ? parent.artWidth : parent.width
            height: page.wide ? parent.height : parent.bandHeight
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
                    ? "image://dither/ramp/" + (page.wide ? "72/14/45/" : "60/14/30/") + Theme.hex(Theme.accentInk) + "/" + art.cols + "/" + art.rows
                    : ""
                Accessible.ignored: true
            }
            Rectangle {
                anchors.right: page.wide ? parent.right : undefined
                anchors.bottom: page.wide ? undefined : parent.bottom
                width: page.wide ? 1 : parent.width
                height: page.wide ? parent.height : 1
                color: Theme.line
            }
        }

        // ── Header and column ───────────────────────────────────────────
        Item {
            id: frame
            x: page.wide ? parent.artWidth : 0
            y: page.wide ? 0 : parent.bandHeight
            width: parent.width - x
            height: parent.height - y

            // The lockup, plain (home = null in the desktop app).
            RowLayout {
                id: header
                x: 24
                y: 16
                height: 44
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
                T.ScrollBar.vertical: ThinScrollBar {}

                ColumnLayout {
                    id: column

                    // lift drives the 4 px rise of a new state.
                    property real lift: 0
                    // pt-10, md:pt-[clamp(32px,10vh,96px)].
                    readonly property real topOffset: page.width >= 768
                        ? Math.max(32, Math.min(96, Math.round(page.height * 0.10)))
                        : 40

                    x: Math.round((flick.width - width) / 2)
                    y: topOffset + lift
                    width: Math.min(440, flick.width - 48)
                    spacing: 0

                    ParallelAnimation {
                        id: enterAnim
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

                    // ── Loading: the invite's geometry, still ──────────
                    ColumnLayout {
                        Layout.fillWidth: true
                        visible: page.step === "loading"
                        spacing: 0
                        Accessible.ignored: true
                        Skeleton {
                            Layout.preferredWidth: 48
                            Layout.preferredHeight: 48
                            Layout.bottomMargin: 24
                        }
                        Item {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 14
                            Layout.bottomMargin: 16
                            Skeleton {
                                anchors.verticalCenter: parent.verticalCenter
                                width: parent.width / 2
                                height: 10
                            }
                        }
                        Item {
                            Layout.fillWidth: true
                            Layout.preferredHeight: Math.round(page.displaySize * 0.9)
                            Skeleton {
                                anchors.verticalCenter: parent.verticalCenter
                                width: parent.width * 0.75
                                height: Math.round(page.displaySize * 0.7)
                            }
                        }
                        Skeleton {
                            Layout.topMargin: 24
                            Layout.fillWidth: true
                            Layout.preferredHeight: 8
                        }
                        Item {
                            Layout.topMargin: 16
                            Layout.fillWidth: true
                            Layout.preferredHeight: 20
                            Skeleton {
                                anchors.verticalCenter: parent.verticalCenter
                                width: 96
                                height: 10
                            }
                        }
                        Skeleton {
                            Layout.topMargin: 32
                            Layout.fillWidth: true
                            Layout.preferredHeight: 48
                        }
                        Skeleton {
                            Layout.topMargin: 12
                            Layout.fillWidth: true
                            Layout.preferredHeight: 48
                        }
                    }

                    // ── The invite (InviteFrame) ───────────────────────
                    // StatusTile tone accent: the server's icon, or its
                    // initial at text-ui-xl semibold.
                    Rectangle {
                        id: serverTile
                        Layout.preferredWidth: 48
                        Layout.preferredHeight: 48
                        Layout.bottomMargin: 24
                        visible: page.showsInvite
                        radius: Theme.radius
                        color: Theme.accentTint
                        clip: true
                        Accessible.ignored: true
                        Image {
                            id: serverIcon
                            anchors.fill: parent
                            visible: status === Image.Ready
                            source: page.showsInvite && page.landing ? page.landing.serverIcon : ""
                            sourceSize: Qt.size(96, 96)
                            fillMode: Image.PreserveAspectCrop
                            asynchronous: true
                        }
                        Text {
                            anchors.centerIn: parent
                            visible: !serverIcon.visible
                            textFormat: Text.PlainText
                            text: page.landing ? page.landing.serverInitial : ""
                            color: Theme.accentInk
                            font: Theme.type.pageTitle
                        }
                    }
                    // PublicKicker: "Invite · <inviter>", the name in ink-2.
                    Row {
                        Layout.fillWidth: true
                        Layout.bottomMargin: 16
                        visible: page.showsInvite
                        spacing: 8
                        Accessible.role: Accessible.StaticText
                        Accessible.name: qsTr("Invite from %1").arg(page.landing ? page.landing.inviter : "")
                        Text {
                            id: kickerLead
                            textFormat: Text.PlainText
                            text: qsTr("Invite ·")
                            color: Theme.ink3
                            font: Theme.type.silk
                            Accessible.ignored: true
                        }
                        Text {
                            width: Math.min(implicitWidth, parent.width - kickerLead.width - parent.spacing)
                            textFormat: Text.PlainText
                            text: page.landing ? page.landing.inviter : ""
                            elide: Text.ElideRight
                            color: Theme.ink2
                            font: Theme.type.silk
                            Accessible.ignored: true
                        }
                    }
                    // PublicTitle size display: Bricolage 800, 44 to 68 px,
                    // leading 0.9, tracking -0.04em, over the full rule.
                    Text {
                        Layout.fillWidth: true
                        visible: page.showsInvite
                        textFormat: Text.PlainText
                        wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                        text: page.serverName
                        color: Theme.ink1
                        font: Qt.font({
                            family: Theme.type.headline.family,
                            pixelSize: page.displaySize,
                            weight: Font.ExtraBold,
                            letterSpacing: -0.04 * page.displaySize,
                            hintingPreference: Theme.type.headline.hintingPreference
                        })
                        lineHeight: 0.9
                        lineHeightMode: Text.ProportionalHeight
                        Accessible.role: Accessible.Heading
                    }
                    DitherRule {
                        Layout.topMargin: 24
                        Layout.preferredWidth: column.width
                        visible: page.showsInvite
                    }
                    // The member count: silkscreen, mt-4, h-5.
                    Row {
                        Layout.topMargin: 16
                        Layout.preferredHeight: 20
                        visible: page.showsInvite
                        spacing: 8
                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: "members"
                            size: Theme.iconXs
                            color: Theme.ink3
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: page.landing ? page.landing.members : ""
                            color: Theme.ink3
                            font: Theme.type.silk
                        }
                    }

                    // ── The other states (PageState) ───────────────────
                    StatusTile {
                        Layout.bottomMargin: 24
                        visible: page.tileIcon.length > 0
                        icon: page.tileIcon
                        tone: page.tileTone
                    }
                    // PublicTitle size xl: 28/32 over a 240 px rule.
                    Text {
                        Layout.fillWidth: true
                        visible: page.titleText.length > 0
                        textFormat: Text.PlainText
                        wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                        text: page.titleText
                        color: Theme.ink1
                        font: Theme.type.dialogTitle
                        lineHeight: Theme.type.dialogTitleLine
                        lineHeightMode: Text.FixedHeight
                        Accessible.role: Accessible.Heading
                    }
                    DitherRule {
                        Layout.topMargin: 16
                        Layout.preferredWidth: Math.min(240, column.width)
                        visible: page.titleText.length > 0
                    }
                    // PublicBody base: 15/22, ink-2, mt-4, max 44ch.
                    Text {
                        Layout.topMargin: 16
                        Layout.fillWidth: true
                        Layout.maximumWidth: 400
                        visible: page.bodyText.length > 0
                        textFormat: Text.PlainText
                        wrapMode: Text.WordWrap
                        text: page.bodyText
                        color: Theme.ink2
                        font: Theme.type.message
                        lineHeight: Theme.type.messageLine
                        lineHeightMode: Text.FixedHeight
                    }

                    // ── PublicActions: 48 px, full width, 12 apart ─────
                    ColumnLayout {
                        Layout.topMargin: 32
                        Layout.fillWidth: true
                        visible: page.step.length > 0 && page.step !== "loading"
                        spacing: 12
                        KitButton {
                            Layout.fillWidth: true
                            visible: page.showsInvite
                            variant: "primary"
                            size: "lg"
                            text: qsTr("Accept invite")
                            loading: page.joining
                            onClicked: if (!page.joining) Invites.accept()
                        }
                        KitButton {
                            Layout.fillWidth: true
                            visible: page.showsInvite
                            variant: "ghost"
                            size: "lg"
                            text: qsTr("No thanks")
                            enabled: !page.joining
                            onClicked: page.close()
                        }
                        KitButton {
                            Layout.fillWidth: true
                            visible: page.step === "joined"
                            variant: "primary"
                            size: "lg"
                            text: qsTr("Open now")
                            onClicked: page.invite.openServer()
                        }
                        KitButton {
                            Layout.fillWidth: true
                            visible: page.step === "member"
                            variant: "primary"
                            size: "lg"
                            text: qsTr("Open %1").arg(page.serverName)
                            onClicked: page.invite.openServer()
                        }
                        KitButton {
                            Layout.fillWidth: true
                            visible: page.step === "error"
                            variant: "primary"
                            size: "lg"
                            text: qsTr("Try again")
                            onClicked: Invites.retry()
                        }
                        KitButton {
                            Layout.fillWidth: true
                            visible: page.step === "invalid" || page.step === "error"
                            variant: page.step === "invalid" ? "primary" : "ghost"
                            size: "lg"
                            text: qsTr("Open Yap")
                            onClicked: page.close()
                        }
                    }
                }
            }
        }
    }

    // ── Parts ───────────────────────────────────────────────────────────

    // A still placeholder (ui/skeleton.tsx rm-skeleton): tint-1, no pulse.
    component Skeleton: Rectangle {
        radius: Theme.radius
        color: Theme.tint1
        Accessible.ignored: true
    }

    // PublicFrame StatusTile: 48 px square, tinted by tone, a 24 px glyph.
    component StatusTile: Rectangle {
        id: tile

        property string icon
        property string tone: "neutral"

        Layout.preferredWidth: 48
        Layout.preferredHeight: 48
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
            color: tile.tone === "accent" ? Theme.accentInk
                : tile.tone === "danger" ? Theme.dangerInk
                : tile.tone === "success" ? Theme.successInk
                : Theme.ink2
        }
    }
}
