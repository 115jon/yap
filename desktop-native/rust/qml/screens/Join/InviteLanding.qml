pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// The invite landing (InviteClient.tsx) as a dialog: loading, the preview
// (the server's tile, "Invite · <inviter>", the server name, its member
// count; Accept invite and No thanks), joining, joined ("Opening the
// server…", it opens after 1.5 s or on Open now), already a member,
// invalid (expired or missing) and the error with Try again. The web's
// signed-out states have no place here: it only opens signed in.
PlateDialog {
    id: dialog

    readonly property var model: JSON.parse(Invites.state)
    readonly property var landing: model.landing ?? null
    readonly property string step: landing ? landing.step : ""
    readonly property bool joining: step === "joining"
    readonly property string serverName: landing ? landing.serverName : ""

    // InviteClient.tsx INVITE_REDIRECT_MS.
    readonly property int redirectMs: 1500

    // The page's poster anatomy in the dialog (PublicFrame.tsx parts): the
    // display title over the dithered rule, the close key, top placed.
    c1: true
    placeTop: true
    rule: step === "preview" || step === "joining"
    kicker: step === "preview" || step === "joining"
        ? qsTr("Invite · %1").arg(landing.inviter)
        : step === "loading" ? qsTr("Invite") : ""
    title: step === "loading" ? qsTr("Loading invite")
        : step === "preview" || step === "joining" ? serverName
        : step === "joined" ? qsTr("Joined %1").arg(serverName)
        : step === "member" ? qsTr("You're already a member")
        : step === "invalid" ? qsTr("This invite isn't valid")
        : step === "error" ? (landing.hasInvite ? qsTr("Couldn't join the server") : qsTr("Couldn't load this invite"))
        : ""
    description: step === "joined" ? qsTr("Opening the server…")
        : step === "member" ? qsTr("You're already in %1.").arg(serverName)
        : step === "invalid" ? (landing.expired
            ? qsTr("It expired or has been used too many times. Ask for a new invite.")
            : qsTr("The link may be mistyped, or the invite was deleted. Ask for a new invite."))
        : step === "error" ? qsTr("Check your connection and try again.")
        : ""
    busy: joining
    onClosed: Invites.closeLanding()
    onLandingChanged: if (!landing && opened) close()
    onStepChanged: {
        if (step === "joined")
            redirect.restart()
        else
            redirect.stop()
    }

    Timer {
        id: redirect
        interval: dialog.redirectMs
        onTriggered: if (dialog.step === "joined") Invites.openServer(dialog.landing.serverId)
    }

    Row {
        visible: dialog.step.length > 0
        width: parent.width
        spacing: 16

        // The status tile: the server's icon or initial on the preview, a
        // glyph for the other states.
        Rectangle {
            id: tile
            readonly property bool showsServer: dialog.step === "preview" || dialog.step === "joining"
            readonly property string glyph: dialog.step === "joined" ? "check"
                : dialog.step === "member" ? "userCheck"
                : dialog.step === "invalid" ? "linkBroken"
                : dialog.step === "error" ? "alert" : ""
            width: 48
            height: 48
            radius: Theme.radius
            color: dialog.step === "loading" ? Theme.tint1
                : dialog.step === "joined" ? Theme.successTint
                : dialog.step === "error" ? Theme.dangerTint
                : tile.showsServer ? Theme.accentTint : Theme.tint2
            Accessible.ignored: true

            Image {
                id: serverIcon
                anchors.fill: parent
                visible: tile.showsServer && status === Image.Ready
                source: tile.showsServer && dialog.landing ? dialog.landing.serverIcon : ""
                sourceSize: Qt.size(96, 96)
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
            }
            Text {
                anchors.centerIn: parent
                visible: tile.showsServer && !serverIcon.visible
                textFormat: Text.PlainText
                text: dialog.landing ? dialog.landing.serverInitial : ""
                color: Theme.accentInk
                font: Theme.type.railInitial
            }
            Icon {
                anchors.centerIn: parent
                visible: tile.glyph.length > 0
                name: tile.glyph
                size: Theme.iconLg
                color: dialog.step === "joined" ? Theme.successInk
                    : dialog.step === "error" ? Theme.dangerInk : Theme.ink2
            }
        }

        // The member count (preview), or the loading placeholders.
        Item {
            width: parent.width - tile.width - parent.spacing
            height: tile.height

            Row {
                anchors.verticalCenter: parent.verticalCenter
                visible: tile.showsServer
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
                    text: dialog.landing ? dialog.landing.members : ""
                    color: Theme.ink3
                    font: Theme.type.silk
                }
            }
            Column {
                anchors.verticalCenter: parent.verticalCenter
                visible: dialog.step === "loading"
                width: parent.width
                spacing: 10
                Accessible.ignored: true
                Rectangle { width: parent.width * 0.5; height: 10; radius: Theme.radius; color: Theme.tint1 }
                Rectangle { width: 96; height: 10; radius: Theme.radius; color: Theme.tint1 }
            }
        }
    }

    footer: [
        KitButton {
            visible: dialog.step === "preview" || dialog.step === "joining"
            variant: "ghost"
            text: qsTr("No thanks")
            enabled: !dialog.joining
            onClicked: dialog.close()
        },
        KitButton {
            visible: dialog.step === "preview" || dialog.step === "joining"
            variant: "primary"
            ground: Theme.ply3
            text: qsTr("Accept invite")
            loading: dialog.joining
            onClicked: if (!dialog.joining) Invites.accept()
        },
        KitButton {
            visible: dialog.step === "joined"
            variant: "primary"
            ground: Theme.ply3
            text: qsTr("Open now")
            onClicked: Invites.openServer(dialog.landing.serverId)
        },
        KitButton {
            visible: dialog.step === "member"
            variant: "primary"
            ground: Theme.ply3
            text: qsTr("Open %1").arg(dialog.serverName)
            onClicked: Invites.openServer(dialog.landing.serverId)
        },
        KitButton {
            visible: dialog.step === "invalid" || dialog.step === "error"
            variant: dialog.step === "invalid" ? "primary" : "ghost"
            ground: Theme.ply3
            text: qsTr("Open Yap")
            onClicked: dialog.close()
        },
        KitButton {
            visible: dialog.step === "error"
            variant: "primary"
            ground: Theme.ply3
            text: qsTr("Try again")
            onClicked: Invites.retry()
        },
        KitButton {
            visible: dialog.step === "loading"
            variant: "ghost"
            text: qsTr("Cancel")
            onClicked: dialog.close()
        }
    ]
}
