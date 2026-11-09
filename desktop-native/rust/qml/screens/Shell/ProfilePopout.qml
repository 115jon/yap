pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Templates as T
import com.yap.native

// The user popout (UserProfilePopover.tsx): the profile card 8 px beside
// what opened it, kept 12 px inside the window. Someone else's card has
// Message (the DM, created when needed) with the friend actions
// (ProfileFriendActions) and, when known, About me and
// their roles in the shown server. The avatar (a key: Tab reaches it) and
// the banner open the full profile (ProfileModal) in its place. Your own card has Edit profile instead
// (the profile editor). A profile that did not load says why under the
// actions (RowError, describeApiError's copy). With Manage roles each role
// chip has a remove key, and the roles row ends in an Add a role key: a
// menu of the server's roles, each checked while the member has it, that
// gives or takes it at once (rolled back with the reason if the server
// refuses). Opens like a popover: scale 0.97 and a
// fade over 200 ms (exit 140). Esc or a press outside closes it.
T.Popup {
    id: popup

    property Item anchorItem: null
    property string side: "right"
    // Who this card was opened for: closing it closes only them.
    property string shownId

    // The server's roles and this member's, for the role menu.
    readonly property var admin: Admin.canManageRoles && visible ? JSON.parse(Admin.state) : null
    readonly property var serverRoles: admin && admin.roles.items ? admin.roles.items.filter(role => !role.isDefault) : []
    readonly property var memberRoleIds: admin ? (admin.members.find(member => member.id === Profile.userId)?.roleIds ?? []) : []
    readonly property string roleError: admin ? (admin.acts["member-roles:" + Profile.userId]?.error ?? "") : ""

    function openFor(userId, anchor, where) {
        anchorItem = anchor
        side = where
        shownId = userId
        Profile.show(userId)
        // UserProfilePopover.tsx loads the relationships for someone else.
        if (userId !== Session.meId)
            Friends.refresh()
        if (Admin.canManageRoles && !Nav.home)
            Admin.load(Nav.activeServerId, "roles")
        open()
        Qt.callLater(place)
    }

    // Beside the anchor on `side`, aligned to its start, inside the window.
    function place() {
        const overlay = T.Overlay.overlay
        if (!anchorItem || !overlay)
            return
        const at = anchorItem.mapToItem(overlay, 0, 0)
        let px = side === "left" ? at.x - width - 8
               : side === "top" ? at.x
               : at.x + anchorItem.width + 8
        let py = side === "top" ? at.y - height - 8 : at.y
        px = Math.max(12, Math.min(px, overlay.width - width - 12))
        py = Math.max(12, Math.min(py, overlay.height - height - 12))
        x = px
        y = py
    }

    parent: T.Overlay.overlay
    // It grows from the edge beside its anchor (the web's
    // `--transform-origin`), not from its centre.
    transformOrigin: side === "left" ? T.Popup.TopRight : side === "top" ? T.Popup.BottomLeft : T.Popup.TopLeft
    width: card.implicitWidth
    height: card.implicitHeight
    padding: 0
    modal: false
    focus: true
    closePolicy: T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutside
    onHeightChanged: if (visible) place()
    onClosed: Profile.close(shownId)

    // The full profile takes over (Shell.qml): close without letting go of
    // the profile it now shows.
    function handOff(userId) {
        if (shownId === userId)
            shownId = ""
        close()
    }
    // The last banner shown (see the card's `banner`).
    property string lastBanner: ""
    Connections {
        target: Profile
        function onBannerChanged() {
            if (Profile.userId.length > 0)
                popup.lastBanner = Profile.banner
        }
        function onUserIdChanged() {
            if (Profile.userId.length > 0)
                popup.lastBanner = Profile.banner
        }
    }

    contentItem: ProfileCard {
        floating: true
        id: card
        name: Profile.name
        username: Profile.username
        avatar: Profile.avatar
        initials: Profile.initials
        pronouns: Profile.pronouns
        presence: Profile.presence
        customStatus: Profile.customStatus
        // The card is kept (Shell.qml): while no profile is shown (closed,
        // or the next one is loading) it keeps the last banner, so
        // reopening that person's card plays it at once instead of
        // decoding it again. Another person's banner replaces it.
        banner: Profile.userId.length === 0 ? popup.lastBanner : Profile.banner
        bannerColor: Profile.bannerColor
        deco: Profile.deco
        effect: Profile.effect
        frame: Profile.frame
        nameStyle: Profile.nameStyle
        platforms: Profile.platforms
        badges: Profile.badges
        animate: popup.visible
        // The avatar and banner open the full profile, which takes over
        // from this card (Shell.qml).
        viewable: Profile.userId.length > 0
        onViewRequested: ShellState.showFullProfile(Profile.userId, popup.anchorItem)
        Accessible.role: Accessible.Dialog
        Accessible.name: qsTr("User profile for %1").arg(Profile.username)

        // Message and the friend actions (ProfileActionRow).
        ProfileFriendActions {
            visible: !Profile.me && Profile.userId.length > 0
            userId: Profile.userId
            username: Profile.username
            name: Profile.name
            onMessageRequested: {
                Nav.openDm(Profile.userId)
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

        RowError {
            above: 0
            text: Profile.loadError
        }

        // "Playing a game", first of the sections (ProfileSections).
        ProfileGame {
            game: Profile.game
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
                width: parent.width
                textFormat: Text.PlainText
                text: Profile.bio
                wrapMode: Text.Wrap
                maximumLineCount: 12
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.description
            }
        }

        Column {
            visible: Profile.roleNames.length > 0 || (Admin.canManageRoles && !Nav.home)
            width: parent.width
            spacing: 8
            Text {
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
                    // ProfileRoleChips, with `onRemove` under Manage roles.
                    delegate: ProfileRoleChip {
                        required property string modelData
                        required property int index
                        name: modelData
                        tone: Profile.roleColors[index] ?? ""
                        removable: Admin.canManageRoles && !Nav.home
                        maxWidth: parent ? parent.width : 300
                        onRemoveRequested: Admin.toggleMemberRole(Profile.userId, Profile.roleIds[index] ?? "")
                    }
                }
                IconKey {
                    id: addRole
                    visible: Admin.canManageRoles && !Nav.home
                    size: 24
                    iconName: "add"
                    tone: "rest"
                    tip: qsTr("Add a role")
                    onClicked: roleMenu.popup(addRole, 0, addRole.height + 4)
                }
            }
            RowError {
                above: 0
                text: popup.roleError
            }
        }

        // Mutual servers and friends (ProfileSections, `limit={6}`).
        ProfileMutuals {
            servers: Profile.mutualServers
            friends: Profile.mutualFriends
            limit: 6
        }

        MenuPanel {
            id: roleMenu
            implicitWidth: 220
            MenuLabel { text: qsTr("Roles") }
            MenuRow {
                visible: popup.admin === null || popup.admin.roles.state !== "ready"
                height: visible ? implicitHeight : 0
                text: qsTr("Loading roles…")
                enabled: false
            }
            MenuRow {
                visible: popup.admin !== null && popup.admin.roles.state === "ready" && popup.serverRoles.length === 0
                height: visible ? implicitHeight : 0
                text: qsTr("No roles to assign yet")
                enabled: false
            }
            Instantiator {
                model: popup.serverRoles
                delegate: MenuRow {
                    required property var modelData
                    text: modelData.name
                    switchable: true
                    on: popup.memberRoleIds.includes(modelData.id)
                    onTriggered: Admin.toggleMemberRole(Profile.userId, modelData.id)
                }
                onObjectAdded: (index, object) => roleMenu.insertItem(index + 3, object)
                onObjectRemoved: (index, object) => roleMenu.removeItem(object)
            }
        }
    }

    background: null

    enter: PresenceIn { duration: Theme.durPopover; fromScale: Theme.scalePopover }
    exit: PresenceOut { duration: Theme.durPopoverExit }
}
