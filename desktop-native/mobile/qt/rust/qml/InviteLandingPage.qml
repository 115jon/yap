pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import Yap.Ui

// An invite link, opened (the Expo app's invite/[code] screen, on the core's
// landing as the desktop's InviteLanding shows it): a page over the pager
// with the server the invite leads to and Accept invite, or Open when you
// are already in it. Its states and copy are Yap.Ui's InviteLandingState:
//   loading   still placeholders in the invite's shape
//   preview   the server's tile, "Invite · <inviter>", its name, its member
//             count; No thanks and Accept invite along the bottom
//   joining   the same, Accept busy and No thanks off
//   joined    "Joined <server>"; the server opens after 1.5 s or on Open now
//   member    "You're already a member", Open <server>
//   invalid   "This invite isn't valid", Open Yap
//   error     Try again and Open Yap
// Opening the server closes the page on its tree (`serverOpened`); No
// thanks, Open Yap and Back close it, except while a join is in flight.
Rectangle {
    id: root

    readonly property bool sheet: false
    readonly property bool isLanding: true
    // A join in flight: Back waits for it.
    readonly property bool holdsBack: invite.joining
    property real backProgress: 0

    signal closeRequested()
    // The landing opened its server (the core has selected it).
    signal serverOpened()

    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: SafeArea.margins.bottom

    readonly property InviteLandingState invite: InviteLandingState {
        onOpening: root.serverOpened()
    }
    readonly property var landing: invite.landing
    readonly property string step: invite.step

    function leave() {
        if (!invite.joining)
            root.closeRequested()
    }
    // The core let the landing go (its server opened from elsewhere).
    onLandingChanged: if (!landing) root.closeRequested()
    // However it closes, the core's landing goes with it (not while a join
    // is in flight: the core keeps that one).
    Component.onDestruction: Invites.closeLanding()

    color: Theme.ply0
    transform: [
        Scale {
            origin.x: root.width / 2
            origin.y: root.height / 2
            xScale: 1 - root.backProgress * 0.08
            yScale: 1 - root.backProgress * 0.08
        },
        Translate { x: root.backProgress * root.width * 0.1 }
    ]

    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Join Server")
    Accessible.description: invite.announcement

    // Nothing under the page takes a touch.
    MouseArea { anchors.fill: parent }

    PageHeader {
        id: header
        z: 1
        title: qsTr("Invite")
        topInset: root.topInset
        leftInset: root.leftInset
        rightInset: root.rightInset
        onBackRequested: root.leave()
    }

    Flickable {
        id: flick
        anchors.top: header.bottom
        anchors.bottom: keys.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: root.leftInset
        anchors.rightMargin: root.rightInset
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        contentWidth: width
        contentHeight: column.implicitHeight + 48

        Column {
            id: column
            x: 24
            y: 24
            width: flick.width - 48
            spacing: 16

            // ── Loading: the invite's shape, still ─────────────────────
            Column {
                visible: root.step === "loading"
                width: parent.width
                spacing: 16
                Accessible.ignored: true
                Skeleton { width: 96; height: 12 }
                Skeleton { width: 88; height: 88 }
                Skeleton { width: Math.min(220, parent.width); height: 24 }
                Skeleton { width: 140; height: 12 }
            }

            // ── The invite ─────────────────────────────────────────────
            // "Invite · <inviter>", the inviter in ink 2.
            Row {
                visible: root.invite.showsInvite
                width: parent.width
                spacing: 8
                Accessible.role: Accessible.StaticText
                Accessible.name: qsTr("Invite from %1").arg(root.landing ? root.landing.inviter : "")
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
                    text: root.landing ? root.landing.inviter : ""
                    elide: Text.ElideRight
                    color: Theme.ink2
                    font: Theme.type.silk
                    Accessible.ignored: true
                }
            }
            // The server's tile: its icon, or its initial.
            Rectangle {
                visible: root.invite.showsInvite
                width: 88
                height: 88
                radius: Theme.radius
                color: Theme.ply3
                border.width: 1
                border.color: Theme.line
                clip: true
                Accessible.ignored: true
                Image {
                    id: serverIcon
                    anchors.fill: parent
                    visible: status === Image.Ready
                    source: root.invite.showsInvite && root.landing ? root.landing.serverIcon : ""
                    sourceSize: Qt.size(176, 176)
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                }
                Text {
                    anchors.centerIn: parent
                    visible: !serverIcon.visible
                    textFormat: Text.PlainText
                    text: root.landing ? root.landing.serverInitial : ""
                    color: Theme.ink1
                    font: Theme.type.dialogTitle
                }
            }
            Text {
                visible: root.invite.showsInvite
                width: parent.width
                textFormat: Text.PlainText
                wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                maximumLineCount: 3
                elide: Text.ElideRight
                text: root.invite.serverName
                color: Theme.ink1
                font: Theme.type.headline
                Accessible.role: Accessible.Heading
            }
            Row {
                visible: root.invite.showsInvite
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
                    text: root.landing ? root.landing.members : ""
                    color: Theme.ink2
                    font: Theme.type.silk
                }
            }

            // ── The other states ───────────────────────────────────────
            Rectangle {
                visible: root.invite.tileIcon.length > 0
                width: 48
                height: 48
                radius: Theme.radius
                readonly property string tone: root.invite.tileTone
                color: tone === "danger" ? Theme.dangerTint
                    : tone === "success" ? Theme.successTint
                    : Theme.tint1
                Accessible.ignored: true
                Icon {
                    anchors.centerIn: parent
                    name: root.invite.tileIcon
                    size: Theme.iconXl
                    color: parent.tone === "danger" ? Theme.dangerInk
                        : parent.tone === "success" ? Theme.successInk
                        : Theme.ink2
                }
            }
            Text {
                visible: root.invite.titleText.length > 0
                width: parent.width
                textFormat: Text.PlainText
                wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                text: root.invite.titleText
                color: Theme.ink1
                font: Theme.type.dialogTitle
                Accessible.role: Accessible.Heading
            }
            Text {
                visible: root.invite.bodyText.length > 0
                width: parent.width
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                text: root.invite.bodyText
                color: Theme.ink2
                font: Theme.type.message
            }
        }
    }

    // ── The keys, along the bottom ─────────────────────────────────────
    Column {
        id: keys
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: 16 + root.leftInset
        anchors.rightMargin: 16 + root.rightInset
        anchors.bottomMargin: 16 + root.bottomInset
        visible: root.step.length > 0 && root.step !== "loading"
        spacing: 8

        Row {
            visible: root.invite.showsInvite
            width: parent.width
            spacing: 8
            KitButton {
                width: (parent.width - parent.spacing) / 2
                variant: "ghost"
                text: qsTr("No thanks")
                enabled: !root.invite.joining
                onClicked: root.leave()
            }
            KitButton {
                width: (parent.width - parent.spacing) / 2
                variant: "primary"
                text: qsTr("Accept invite")
                loading: root.invite.joining
                onClicked: {
                    if (root.invite.joining)
                        return
                    Native.haptic(Native.Confirm)
                    Invites.accept()
                }
            }
        }
        KitButton {
            visible: root.step === "joined"
            width: parent.width
            variant: "primary"
            text: qsTr("Open now")
            onClicked: root.invite.openServer()
        }
        KitButton {
            visible: root.step === "member"
            width: parent.width
            variant: "primary"
            text: qsTr("Open %1").arg(root.invite.serverName)
            onClicked: root.invite.openServer()
        }
        KitButton {
            visible: root.step === "error"
            width: parent.width
            variant: "primary"
            text: qsTr("Try again")
            onClicked: Invites.retry()
        }
        KitButton {
            visible: root.step === "invalid" || root.step === "error"
            width: parent.width
            variant: root.step === "invalid" ? "primary" : "ghost"
            text: qsTr("Open Yap")
            onClicked: root.leave()
        }
    }

    // A still placeholder: tint 1, no pulse.
    component Skeleton: Rectangle {
        radius: Theme.radius
        color: Theme.tint1
        Accessible.ignored: true
    }
}
