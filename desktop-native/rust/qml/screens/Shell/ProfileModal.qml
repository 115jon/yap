pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Templates as T
import com.yap.native

// The full profile (UserProfileModal.tsx): what every "Profile" menu row
// opens (Friends, DMs, the member list, a message, a voice occupant). The
// shared ProfileCard at modal size, 560 wide (max-w-560, 16 px from the
// window's sides), anchored at max(48, 12 % of the window) and growing
// downward, so loading or the block confirm never recentres it; one
// scroller, the window less 24 px under it. The dialog's 40 px tint-2
// close sits on the banner 16 in. The card has the actions
// (ProfileActionRow: only Try again when the profile failed to load), the
// load error, About me, Mutual servers and friends (8 each) and Member
// since, then the block confirm. Never for yourself (ChatPageClient). Esc
// backs out of the block confirm first. The name and decorations play
// while it is open (data-decoration-play). Opens like a dialog: scale 0.97
// and a fade over 200 ms.
T.Popup {
    id: popup

    property string shownId

    function openFor(userId) {
        if (!userId || userId.length === 0 || userId === Session.meId)
            return
        shownId = userId
        Profile.show(userId)
        Friends.refresh()
        open()
    }

    parent: T.Overlay.overlay
    readonly property real topAt: Math.max(48, (parent ? parent.height : 800) * 0.12)
    width: Math.min(560, (parent ? parent.width : 600) - 32)
    height: Math.min(card.implicitHeight, (parent ? parent.height : 800) - topAt - 24)
    x: Math.round(((parent ? parent.width : 600) - width) / 2)
    y: topAt
    padding: 0
    modal: true
    dim: true
    focus: true
    closePolicy: actions.confirmBlock ? T.Popup.CloseOnPressOutside
        : T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutside
    onVisibleChanged: ShellState.modalCount += visible ? 1 : -1
    onClosed: Profile.close(shownId)

    Overlay.modal: Scrim {}

    // Esc backs out of the block confirm before the profile.
    Shortcut {
        sequence: "Esc"
        enabled: popup.visible && actions.confirmBlock
        onActivated: actions.confirmBlock = false
    }

    background: null

    contentItem: Flickable {
        id: scroller
        clip: true
        contentWidth: width
        contentHeight: card.implicitHeight
        boundsBehavior: Flickable.StopAtBounds
        interactive: contentHeight > height
        ScrollBar.vertical: ThinScrollBar {}
        Accessible.role: Accessible.Dialog
        Accessible.name: qsTr("%1's profile").arg(Profile.name)
        Accessible.description: "@" + Profile.username + ", " + Theme.presenceWord(Profile.presence)

        ProfileCard {
            id: card
            width: scroller.width
            size: "modal"
            floating: true
            name: Profile.name
            username: Profile.username
            avatar: Profile.avatar
            initials: Profile.initials
            pronouns: Profile.pronouns
            presence: Profile.presence
            customStatus: Profile.customStatus
            banner: Profile.banner
            bannerColor: Profile.bannerColor
            deco: Profile.deco
            effect: Profile.effect
            frame: Profile.frame
            nameStyle: Profile.nameStyle
            animate: popup.visible

            // ProfileActionRow, or Try again alone after a failed load.
            ProfileFriendActions {
                id: actions
                visible: !Profile.me && Profile.userId.length > 0 && Profile.loadError.length === 0
                userId: Profile.userId
                username: Profile.username
                name: Profile.name
                onMessageRequested: {
                    Nav.openDm(Profile.userId)
                    popup.close()
                }
            }
            KitButton {
                visible: Profile.loadError.length > 0
                variant: "secondary"
                iconName: "refresh"
                text: qsTr("Try again")
                onClicked: Profile.show(Profile.userId)
            }
            RowError {
                above: 0
                text: Profile.loadError
            }

            Column {
                visible: Profile.bio.length > 0
                width: parent.width
                spacing: 8
                Text {
                    text: qsTr("About me")
                    color: Theme.ink2
                    font: Theme.type.captionStrong
                    Accessible.role: Accessible.Heading
                }
                Text {
                    // max-w-[62ch] at 13 px.
                    width: Math.min(parent.width, 62 * 7.2)
                    textFormat: Text.PlainText
                    text: Profile.bio
                    wrapMode: Text.Wrap
                    color: Theme.ink1
                    font: Theme.type.description
                }
            }

            ProfileMutuals {
                servers: Profile.mutualServers
                friends: Profile.mutualFriends
                limit: 8
            }

            Column {
                readonly property var since: Profile.memberSince.length > 0 ? JSON.parse(Profile.memberSince) : null
                visible: since !== null
                width: parent.width
                spacing: 8
                Text {
                    textFormat: Text.PlainText
                    text: parent.since ? parent.since.label : ""
                    color: Theme.ink2
                    font: Theme.type.captionStrong
                    Accessible.role: Accessible.Heading
                }
                Text {
                    textFormat: Text.PlainText
                    text: parent.since ? parent.since.value : ""
                    color: Theme.ink1
                    font: Theme.type.description
                }
            }
        }

        // The dialog's close key: 40 px tint-2, the 20 px X, 16 in.
        IconKey {
            x: card.width - card.inSide - width - 16
            y: card.inTop + 16
            z: 3
            size: 40
            iconName: "close"
            tone: "toggled"
            // The Bold X on tint-2 (the toggled tone's Fill glyph is the X
            // in a square).
            iconVariant: "idle"
            tip: qsTr("Close")
            onClicked: popup.close()
        }
    }

    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
            NumberAnimation { property: "scale"; from: Theme.scaleIn; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
        }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: Theme.dur3Exit }
    }
}
