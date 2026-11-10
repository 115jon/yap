pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Templates as T
import com.yap.native

// The full profile (UserProfileModal.tsx): what the popout's avatar and
// banner and every "Profile" menu row open (Friends, DMs, the member list,
// a message, a voice occupant). The shared ProfileCard at modal size (a
// 176 px banner, the 112 px avatar, the name at 28/600), 376 wide beside
// the board column (BoardPanel: Board, Activity, Wishlist), about 1120 in
// all (16 px from the window's sides) and stacked under 720, anchored at
// max(48, 12 % of the window) and growing downward, so loading or the block
// confirm never recentres it; side by side each column scrolls on its own
// and the board column runs the full height, the window less 24 px under
// it. The dialog's 40 px tint-2 close sits 16 in from the modal's top
// right. Someone else's has
// the actions (ProfileActionRow with Call; only Try again when the profile
// failed to load); your own has Edit profile. Then the load error, Playing
// a game, About me (the message grammar, markdown.rs), Roles in the shown
// server, Mutual servers and friends (8 each) and Member since, then the
// block confirm. Esc backs out of the block confirm first. The name and
// decorations play while it is open (data-decoration-play). Opens like a
// dialog: scale 0.97 and a fade over 200 ms. Closing gives focus back to
// what had it when it opened.
T.Popup {
    id: popup

    property string shownId
    // Focus before it opened, given back on close.
    property Item returnFocus: null
    // Spoilers in the bio the reader revealed (MessageDelegate's rule).
    property var revealedSpoilers: ({})

    function openFor(userId) {
        if (!userId || userId.length === 0)
            return
        shownId = userId
        revealedSpoilers = {}
        returnFocus = parent ? parent.Window.activeFocusItem : null
        Profile.show(userId)
        Board.open(userId)
        if (userId !== Session.meId)
            Friends.refresh()
        open()
    }

    function shadeSpoilers(html, hot) {
        return html.replace(/href="spoiler:(\d+)" style="color:%SPOILERFG%; background-color:%SPOILERBG%/g,
            (match, n) => {
                const shown = revealedSpoilers[n] === true
                if (!shown && hot !== "spoiler:" + n)
                    return match
                const fg = shown ? Theme.richRevealed["%SPOILERFG%"] : Theme.solid(Theme.lineStrong)
                const bg = shown ? Theme.richRevealed["%SPOILERBG%"] : fg
                return "href=\"spoiler:" + n + "\" style=\"color:" + fg + "; background-color:" + bg
            })
    }

    parent: T.Overlay.overlay
    readonly property real topAt: Math.max(48, (parent ? parent.height : 800) * 0.12)
    // Card (376) beside the board column, about 1120 in all; stacked, one
    // scroller, under 720 (UserProfileModal.tsx).
    width: Math.min(1120, (parent ? parent.width : 680) - 32)
    readonly property real room: (parent ? parent.height : 800) - topAt - 24
    readonly property bool wide: width >= 720
    readonly property int cardWidth: wide ? 376 : width
    height: wide ? Math.min(room, Math.max(card.implicitHeight, panel.implicitHeight + 2))
        : Math.min(room, card.implicitHeight + panel.implicitHeight + 1)
    x: Math.round(((parent ? parent.width : 680) - width) / 2)
    y: topAt
    padding: 0
    modal: true
    dim: true
    focus: true
    // With unsaved board edits nothing closes it on its own: Esc, the scrim
    // and the close key go through the board's discard confirm.
    closePolicy: actions.confirmBlock ? T.Popup.CloseOnPressOutside
        : panel.unsaved ? T.Popup.NoAutoClose
        : T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutside

    // Closes now, or asks first while board edits are unsaved.
    function requestClose() {
        if (panel.mayClose())
            close()
    }
    onVisibleChanged: ShellState.modalCount += visible ? 1 : -1
    onClosed: {
        Profile.close(shownId)
        Board.close()
        const back = returnFocus
        returnFocus = null
        if (back && back.visible)
            back.forceActiveFocus()
    }

    Overlay.modal: Scrim {
        stepsBack: panel.unsaved && !actions.confirmBlock
        onTapped: popup.requestClose()
    }
    Shortcut {
        sequence: "Esc"
        enabled: popup.visible && !actions.confirmBlock && panel.unsaved
        onActivated: popup.requestClose()
    }

    // Esc backs out of the block confirm before the profile.
    Shortcut {
        sequence: "Esc"
        enabled: popup.visible && actions.confirmBlock
        onActivated: actions.confirmBlock = false
    }

    background: null

    contentItem: Flickable {
        // Stacked: this one scrolls both columns. Side by side each column
        // scrolls on its own and this one stays put (it takes no wheel).
        id: outer
        clip: !popup.wide
        contentWidth: width
        contentHeight: popup.wide ? height : card.implicitHeight + panel.implicitHeight + 1
        boundsBehavior: Flickable.StopAtBounds
        interactive: !popup.wide && contentHeight > height
        ScrollBar.vertical: ThinScrollBar { visible: !popup.wide && outer.contentHeight > outer.height }

    Flickable {
        id: scroller
        width: popup.cardWidth
        height: popup.wide ? Math.min(card.implicitHeight, outer.height) : card.implicitHeight
        clip: true
        contentWidth: width
        contentHeight: card.implicitHeight
        boundsBehavior: Flickable.StopAtBounds
        interactive: popup.wide && contentHeight > height
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
            badges: Profile.badges
            animate: popup.visible

            // ProfileActionRow, or Try again alone after a failed load.
            ProfileFriendActions {
                id: actions
                visible: !Profile.me && Profile.userId.length > 0 && Profile.loadError.length === 0
                userId: Profile.userId
                username: Profile.username
                name: Profile.name
                callable: true
                onMessageRequested: {
                    Nav.openDm(Profile.userId)
                    popup.close()
                }
                onCallRequested: {
                    ShellState.startCallWithRequested(Profile.userId, Profile.name)
                    popup.close()
                }
            }
            // ProfileActionRow `selfAction`.
            KitButton {
                visible: Profile.me && Profile.userId.length > 0
                variant: "primary"
                ground: Theme.ply3
                iconName: "edit"
                text: qsTr("Edit profile")
                onClicked: {
                    popup.close()
                    SettingsNav.profileEditorRequested()
                }
            }
            KitButton {
                visible: Profile.loadError.length > 0 && !Profile.me
                variant: "secondary"
                iconName: "refresh"
                text: qsTr("Try again")
                onClicked: Profile.show(Profile.userId)
            }
            RowError {
                above: 0
                text: Profile.loadError
            }

            ProfileGame {
                game: Profile.game
            }

            Column {
                visible: Profile.bioHtml.length > 0
                width: parent.width
                spacing: 8
                Text {
                    textFormat: Text.PlainText
                    text: qsTr("About me")
                    color: Theme.ink2
                    font: Theme.type.captionStrong
                    Accessible.role: Accessible.Heading
                }
                Text {
                    id: bio
                    // max-w-[62ch] at 13 px.
                    width: Math.min(parent.width, 62 * 7.2)
                    // yap-core markdown.rs: the message grammar as rich text.
                    textFormat: Text.RichText
                    text: Theme.richText(popup.shadeSpoilers(Theme.underlineLink(Profile.bioHtml, hotLink), hotLink), false, false)
                    // The hovered link, underlined (MessageDelegate's rule).
                    property string hotLink: ""
                    onHoveredLinkChanged: hotLink = hoveredLink
                    wrapMode: Text.Wrap
                    color: Theme.ink1
                    font: Theme.type.description
                    // The bio is untrusted: only web and mail links open.
                    onLinkActivated: link => {
                        if (link.startsWith("spoiler:"))
                            popup.revealedSpoilers = Object.assign({}, popup.revealedSpoilers, { [link.slice(8)]: true })
                        else if (link.startsWith("user:"))
                            ShellState.showFullProfile(link.slice(5), bio)
                        else if (Invites.openLink(link))
                            return
                        else if (/^(https?:|mailto:)/i.test(link))
                            Qt.openUrlExternally(link)
                    }
                    HoverHandler {
                        cursorShape: bio.hotLink.length > 0
                            && !(bio.hotLink.startsWith("spoiler:") && popup.revealedSpoilers[bio.hotLink.slice(8)] === true)
                            ? Qt.PointingHandCursor : Qt.ArrowCursor
                    }
                }
            }

            // Their roles in the shown server (ProfileRoleChips, read only).
            Column {
                visible: Profile.roleNames.length > 0
                width: parent.width
                spacing: 8
                Text {
                    textFormat: Text.PlainText
                    text: qsTr("Roles")
                    color: Theme.ink2
                    font: Theme.type.captionStrong
                    Accessible.role: Accessible.Heading
                }
                Flow {
                    width: parent.width
                    spacing: 6
                    Repeater {
                        model: Profile.roleNames
                        delegate: ProfileRoleChip {
                            required property string modelData
                            required property int index
                            name: modelData
                            tone: Profile.roleColors[index] ?? ""
                            maxWidth: parent ? parent.width : 300
                        }
                    }
                }
            }

            ProfileMutuals {
                servers: Profile.me ? "" : Profile.mutualServers
                friends: Profile.me ? "" : Profile.mutualFriends
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

    }

    // The board column (ProfilePanel): beside the card, or under it.
    Flickable {
        id: panelScroller
        x: popup.wide ? popup.cardWidth : 0
        y: popup.wide ? 0 : card.implicitHeight + 1
        width: popup.width - x
        // Side by side it runs the modal's full height, as the card does.
        height: popup.wide ? outer.height : panel.implicitHeight + 1
        clip: true
        contentWidth: width
        contentHeight: panel.height + 2
        boundsBehavior: Flickable.StopAtBounds
        interactive: popup.wide && contentHeight > height
        ScrollBar.vertical: ThinScrollBar {}

        BoardPanel {
            id: panel
            x: popup.wide ? 0 : 1
            y: 1
            width: panelScroller.width - (popup.wide ? 1 : 2)
            height: Math.max(implicitHeight, popup.wide ? panelScroller.height - 2 : 0)
            // The close key sits at the header's right end side by side.
            closeReserve: popup.wide ? 64 : 0
            ownerName: Profile.name
            ownerAvatar: Profile.avatar
            ownerInitials: Profile.initials
            scroller: panelScroller
            onCloseRequested: popup.close()
        }
    }
    // Its edge: line-strong, none against the card when side by side.
    Rectangle {
        x: panelScroller.x
        y: panelScroller.y
        width: panelScroller.width
        height: panelScroller.height
        color: "transparent"
        border.width: 1
        border.color: Theme.lineStrong
        Rectangle {
            visible: popup.wide
            width: 1
            height: parent.height
            color: Theme.ply3
        }
    }

    // The dialog's close key (40 px tint-2, the 20 px X): the modal's top
    // right, 16 in, as on the web: over the board column's tabs side by
    // side (clear of Edit board), over the card's banner when stacked.
    IconKey {
        x: popup.width - width - 16 - (popup.wide ? 0 : card.inSide)
        y: 16 + (popup.wide ? 0 : card.inTop)
        z: 5
        size: 40
        iconName: "close"
        tone: "toggled"
        // The Bold X on tint-2 (the toggled tone's Fill glyph is the X
        // in a square).
        iconVariant: "idle"
        tip: qsTr("Close")
        onClicked: popup.requestClose()
    }
    }

    enter: PresenceIn { fromScale: Theme.scaleDialog }
    exit: PresenceOut { toScale: Theme.scaleDialogOut }
}
