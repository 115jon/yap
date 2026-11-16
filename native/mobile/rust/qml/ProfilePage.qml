pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Someone's profile, a full-height sheet over the pager, on the desktop's
// Profile object (show, close): the banner, the avatar with presence, the
// name and handle, their badges, their status, then the actions (Edit
// profile on your own), About, roles, mutual servers and friends, and
// Member since. Opened from a message's avatar or name, a member row, a DM
// row's avatar or a DM's header.
//
// Their collectibles are the desktop card's (Yap.Ui): the frame around the
// page (its back art under it, its front art over it, the page inset by the
// frame's footprint) and the profile effect over it, one pass and then its
// still (only the still under reduced motion). A video banner shows its
// first frame (InlineVideo's poster): the phone plays video only in the
// player sheet or full screen, so nothing decodes on a page that stays open.
//
// Their board (Yap.Ui BoardCore and BoardRack, the desktop's widgets) sits
// under the profile on the desktop column's ply-3: Board · Activity ·
// Wishlist. On your own, Edit board opens the board editor
// (BoardEditPage) over the profile.
//
// The actions follow the relationship (Yap.Ui ProfileFriendCore): Message
// or Accept across the page, then Add friend, Cancel request, Decline or
// Remove friend beside Block; someone blocked has Unblock and the notice,
// and no Message. Remove and Block ask first, in place.
//
// TODO: present it in the shared BottomSheet (drag to dismiss) once that
// is on main; until then it is a page of the stack that rises from the
// bottom edge.
Rectangle {
    id: root

    required property string userId

    // The stack rises and drops a sheet instead of sliding a page in.
    readonly property bool sheet: true
    // The predictive back gesture's progress (0..1): the sheet follows the
    // finger down before the pop commits.
    property real backProgress: 0

    signal closeRequested()
    // Message: the DM with them, opened by the caller.
    signal messageRequested(string userId)
    // Edit profile (your own), opened by the caller.
    signal profileEditRequested()

    readonly property bool loaded: Profile.userId === userId
    // Read once for the page: content that scrolls or sits at an inset
    // would feed its own position back into SafeArea.
    // From the stack rather than the page, which rises through the insets.
    readonly property Item frame: StackView.view ?? root
    readonly property real topInset: frame.SafeArea.margins.top
    readonly property real leftInset: frame.SafeArea.margins.left
    readonly property real rightInset: frame.SafeArea.margins.right
    readonly property real bottomInset: frame.SafeArea.margins.bottom
    // `{label, value}`: when they joined this server, or made the account.
    readonly property var since: {
        if (!loaded || Profile.memberSince.length === 0)
            return null
        try {
            return JSON.parse(Profile.memberSince)
        } catch (e) {
            return null
        }
    }
    readonly property int bannerHeight: 132
    readonly property int avatarSize: 88
    // The frame's footprint (fractions of the page width), when they wear one.
    readonly property var frameInfo: {
        if (!loaded || Profile.frame.length === 0)
            return null
        try {
            return JSON.parse(Profile.frame)
        } catch (e) {
            return null
        }
    }
    readonly property bool framed: frameInfo !== null
    readonly property real inSide: framed ? frameInfo.side * width : 0
    readonly property real inTop: framed ? frameInfo.top * width : 0
    readonly property real inBottom: framed ? frameInfo.bottom * width : 0
    // A video file (ProfileVideoLayer.isVideo on the desktop): an extension,
    // or the type the core put in the fragment (`#video/webm`).
    readonly property string banner: loaded ? Profile.banner : ""
    readonly property bool videoBanner: /\.(mp4|webm|mov|m4v|ogv)($|[?#])|#video\//i.test(banner)
    // Remove friend, asked in place (Block asks through `friend`).
    property bool confirmRemove: false
    readonly property string asking: friend.confirmBlock ? "block" : confirmRemove ? "remove" : ""

    // Their relationship and its actions.
    ProfileFriendCore {
        id: friend
        userId: root.userId
        username: root.loaded ? Profile.username : ""
        name: root.loaded ? Profile.name : ""
        onRelationChanged: if (relation !== "friend") root.confirmRemove = false
    }
    // Your own board's editor, over this page.
    function editBoard() {
        if (!board.shown || !board.isOwner)
            return
        Native.haptic(Native.Tick)
        // The page starts the editor (Board.edit) as it opens.
        Router.open("BoardEditPage", {
            ownerName: Profile.name,
            ownerAvatar: Profile.avatar,
            ownerInitials: Profile.initials
        }, { key: root.userId })
    }

    function ask(action) {
        // A new step starts clean (not with the last attempt's failure).
        Friends.clear(userId)
        confirmRemove = action === "remove"
        friend.confirmBlock = action === "block"
    }

    color: Theme.ply1
    transform: Translate { y: root.backProgress * root.height * 0.12 }

    Component.onCompleted: {
        Profile.show(userId)
        // The relationship for the actions (UserProfilePopover.tsx).
        if (userId !== Session.meId)
            Friends.refresh()
        Board.open(userId)
    }
    // Back on top of the stack after a profile opened from this one: the
    // board is theirs again.
    StackView.onActivated: if (board.shownId !== userId) Board.open(userId)
    Component.onDestruction: {
        Profile.close(userId)
        if (board.shownId === userId)
            Board.close()
    }

    // The board's state and tab (shared with the desktop's BoardPanel).
    BoardCore {
        id: board
        readonly property string shownId: st.userId ?? ""
        readonly property bool shown: shownId === root.userId
        ownerName: root.loaded ? Profile.name : ""
        ownerAvatar: root.loaded ? Profile.avatar : ""
        ownerInitials: root.loaded ? Profile.initials : ""
    }

    Flickable {
        id: scroll
        anchors.fill: parent
        contentHeight: framedBox.y + framedBox.height + root.bottomInset + 24
        boundsBehavior: Flickable.StopAtBounds

        // The page in its frame: under the status bar when framed, and the
        // banner runs up under it when not.
        Item {
            id: framedBox
            y: root.framed ? root.topInset : 0
            width: scroll.width
            height: root.inTop + body.height + root.inBottom

            ProfileFrameLayer {
                visible: root.framed
                anchors.fill: parent
                frame: root.framed ? Profile.frame : ""
                order: "back"
            }
            // The page's own ground over the frame's back art.
            Rectangle {
                visible: root.framed
                x: root.inSide
                y: root.inTop
                width: body.width
                height: body.height
                radius: Theme.radius
                color: root.color
                border.width: 1
                border.color: Theme.lineStrong
            }

            Column {
                id: body
                x: root.inSide
                y: root.inTop
                width: parent.width - 2 * root.inSide

                Rectangle {
                    id: bannerArea
                    width: parent.width
                    height: (root.framed ? 0 : root.topInset) + root.bannerHeight
                    color: root.loaded && Profile.bannerColor.length > 0 ? Profile.bannerColor : Theme.accentTint
                    clip: true
                    Image {
                        anchors.fill: parent
                        visible: status === Image.Ready
                        source: root.videoBanner ? "" : root.banner
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                        sourceSize.width: 1080
                    }
                    // A video banner: its first frame, still.
                    Loader {
                        anchors.fill: parent
                        active: root.videoBanner
                        sourceComponent: InlineVideo {
                            source: root.banner.replace(/#.*$/, "")
                            preview: true
                            bare: true
                            Accessible.ignored: true
                        }
                    }
                }

                Item {
                    id: identity
                    width: parent.width
                    height: names.y + names.height + 16

                    // The avatar straddles the banner's foot on a ply-1 plate.
                    Rectangle {
                        id: plate
                        x: 16 + root.leftInset
                        y: -root.avatarSize / 2
                        width: root.avatarSize + 8
                        height: root.avatarSize + 8
                        radius: Theme.radius + 2
                        color: root.color
                        PresenceAvatar {
                            anchors.centerIn: parent
                            size: root.avatarSize
                            initials: root.loaded ? Profile.initials : ""
                            source: root.loaded ? Profile.avatar : ""
                            deco: root.loaded ? Profile.deco : ""
                            presence: root.loaded ? Profile.presence : ""
                            ground: root.color
                        }
                    }

                    Column {
                        id: names
                        anchors.left: parent.left
                        anchors.leftMargin: 20 + root.leftInset
                        anchors.right: parent.right
                        anchors.rightMargin: 20 + root.rightInset
                        y: root.avatarSize / 2 + 16
                        spacing: 4
                        visible: root.loaded

                        DisplayName {
                            width: parent.width
                            text: Profile.name
                            nameStyle: Profile.nameStyle
                            color: Theme.ink1
                            font: Theme.type.pageTitle
                            ground: root.color
                            playing: true
                        }
                        Text {
                            width: parent.width
                            textFormat: Text.PlainText
                            text: Profile.pronouns.length > 0 ? "@%1 · %2".arg(Profile.username).arg(Profile.pronouns)
                                : "@" + Profile.username
                            elide: Text.ElideRight
                            color: Theme.ink2
                            font: Theme.type.description
                        }
                        // A tap shows a badge's name, what it is for and when
                        // it was earned. The drawings line up with the text,
                        // not their keys' 2 px padding.
                        UserBadges {
                            x: -2
                            width: parent.width + 2
                            badges: Profile.badges
                            size: "lg"
                        }
                        Row {
                            spacing: 8
                            topPadding: 4
                            Led {
                                anchors.verticalCenter: word.verticalCenter
                                lit: Profile.presence !== "offline"
                                color: Theme.presenceLed(Profile.presence)
                            }
                            Text {
                                id: word
                                text: Theme.presenceWord(Profile.presence)
                                color: Theme.presenceInk(Profile.presence)
                                font: Theme.type.silk
                            }
                        }
                        Text {
                            width: parent.width
                            visible: Profile.customStatus.length > 0
                            textFormat: Text.PlainText
                            text: Profile.customStatus
                            wrapMode: Text.Wrap
                            color: Theme.ink2
                            font: Theme.type.description
                        }
                        Loader {
                            width: parent.width
                            active: Profile.game.length > 0
                            visible: active
                            sourceComponent: GameLine {
                                game: Profile.game
                            }
                        }
                    }

                    Spinner {
                        anchors.horizontalCenter: parent.horizontalCenter
                        y: root.avatarSize / 2 + 40
                        running: !root.loaded && Profile.loadError.length === 0
                    }
                }

                Text {
                    x: 20 + root.leftInset
                    width: parent.width - 2 * x
                    visible: Profile.loadError.length > 0
                    bottomPadding: 16
                    textFormat: Text.PlainText
                    text: Profile.loadError
                    wrapMode: Text.Wrap
                    color: Theme.dangerInk
                    font: Theme.type.description
                }

                Item {
                    width: parent.width
                    height: editKey.height + 16
                    visible: root.loaded && Profile.me
                    KitButton {
                        id: editKey
                        x: 20 + root.leftInset
                        width: parent.width - 2 * x
                        variant: "secondary"
                        iconName: "edit"
                        text: qsTr("Edit profile")
                        onClicked: root.profileEditRequested()
                    }
                }
                // ProfileActionRow: the primary across the page, then the rest.
                Column {
                    id: actions
                    x: 20 + root.leftInset
                    width: parent.width - 2 * x
                    visible: root.loaded && !Profile.me && Profile.loadError.length === 0
                        && friend.relation !== "self"
                    spacing: 8
                    bottomPadding: 16

                    KitButton {
                        visible: !friend.blocked
                        width: parent.width
                        variant: "primary"
                        ground: root.color
                        iconName: friend.relation === "incoming" ? "check" : "chat"
                        text: friend.relation === "incoming" ? qsTr("Accept") : qsTr("Message")
                        loading: friend.act.busy === "accept"
                        enabled: !friend.pending
                        onClicked: {
                            Native.haptic(Native.Confirm)
                            if (friend.relation === "incoming")
                                friend.run("accept")
                            else
                                root.messageRequested(root.userId)
                        }
                    }

                    Row {
                        id: keys
                        readonly property int count: friend.blocked ? 1 : friend.relation === "incoming" ? 3 : 2
                        readonly property real share: (width - spacing * (count - 1)) / count
                        visible: root.asking.length === 0
                        width: parent.width
                        spacing: 8

                        KitButton {
                            visible: friend.relation === "incoming"
                            width: keys.share
                            variant: "secondary"
                            iconName: "chat"
                            text: qsTr("Message")
                            enabled: !friend.pending
                            onClicked: root.messageRequested(root.userId)
                        }
                        KitButton {
                            visible: friend.relation === "incoming"
                            width: keys.share
                            variant: "ghost"
                            text: qsTr("Decline")
                            loading: friend.act.busy === "decline"
                            enabled: !friend.pending
                            onClicked: friend.run("decline")
                        }
                        KitButton {
                            visible: friend.relation === "outgoing"
                            width: keys.share
                            variant: "secondary"
                            text: qsTr("Cancel request")
                            loading: friend.act.busy === "cancel"
                            enabled: !friend.pending
                            onClicked: friend.run("cancel")
                        }
                        KitButton {
                            visible: friend.relation === "none"
                            width: keys.share
                            variant: "secondary"
                            iconName: "invite"
                            text: qsTr("Add friend")
                            loading: friend.act.busy === "add"
                            enabled: !friend.pending
                            onClicked: {
                                Native.haptic(Native.Confirm)
                                friend.run("add")
                            }
                        }
                        KitButton {
                            visible: friend.relation === "friend"
                            width: keys.share
                            variant: "secondary"
                            iconName: "userRemove"
                            text: qsTr("Remove friend")
                            enabled: !friend.pending
                            onClicked: root.ask("remove")
                        }
                        KitButton {
                            visible: !friend.blocked
                            width: keys.share
                            variant: "destructive"
                            iconName: "ban"
                            text: qsTr("Block")
                            Accessible.name: qsTr("Block %1").arg(friend.name)
                            enabled: !friend.pending
                            onClicked: root.ask("block")
                        }
                        KitButton {
                            visible: friend.blocked
                            width: keys.share
                            variant: "secondary"
                            text: qsTr("Unblock")
                            Accessible.name: qsTr("Unblock %1").arg(friend.name)
                            loading: friend.act.busy === "unblock"
                            enabled: !friend.pending
                            onClicked: friend.run("unblock")
                        }
                    }

                    // The blocked notice (ProfileNotice).
                    Rectangle {
                        visible: friend.blocked
                        width: parent.width
                        height: notice.implicitHeight + 20
                        radius: Theme.radius
                        color: Theme.tint1
                        Icon {
                            x: 12
                            y: 11
                            name: "ban"
                            size: Theme.iconSm
                            color: Theme.ink3
                        }
                        Text {
                            id: notice
                            x: 36
                            y: 10
                            width: parent.width - 48
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            text: friend.blockedNotice
                            color: Theme.ink2
                            font: Theme.type.description
                        }
                    }

                    // Remove or Block, asked in place.
                    Loader {
                        width: parent.width
                        active: root.asking.length > 0
                        visible: active
                        sourceComponent: InlineConfirm {
                            consequence: root.asking === "block" ? friend.blockConsequence
                                : qsTr("%1 will be removed from your friends. You can send a new request later.").arg(friend.name)
                            confirmLabel: root.asking === "block" ? qsTr("Block") : qsTr("Remove")
                            dismissLabel: qsTr("Keep")
                            busy: friend.act.busy === root.asking
                            error: friend.act.failed === root.asking ? friend.act.error : ""
                            onCancelled: root.ask("")
                            onConfirmed: friend.run(root.asking)
                        }
                    }

                    // A failed action (a confirmed one's shows in its strip).
                    Text {
                        width: parent.width
                        visible: friend.act.error.length > 0 && friend.act.failed !== root.asking
                        textFormat: Text.PlainText
                        text: friend.act.error
                        wrapMode: Text.Wrap
                        color: Theme.dangerInk
                        font: Theme.type.caption
                        Accessible.role: Accessible.AlertMessage
                        Accessible.name: text
                    }
                }

                Section {
                    title: qsTr("About")
                    visible: root.loaded && Profile.bio.length > 0
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: Profile.bio
                        wrapMode: Text.Wrap
                        color: Theme.ink1
                        font: Theme.type.message
                    }
                }

                Section {
                    title: qsTr("Roles")
                    visible: root.loaded && Profile.roleNames.length > 0
                    Flow {
                        width: parent.width
                        spacing: 6
                        Repeater {
                            model: root.loaded ? Profile.roleNames : []
                            Rectangle {
                                id: chip
                                required property int index
                                required property string modelData
                                readonly property string tone: Profile.roleColors[index] ?? ""
                                width: Math.min(chipRow.implicitWidth + 16, parent.width)
                                height: 28
                                radius: Theme.radius
                                color: Theme.tint1
                                border.width: 1
                                border.color: Theme.line
                                Row {
                                    id: chipRow
                                    x: 8
                                    anchors.verticalCenter: parent.verticalCenter
                                    spacing: 6
                                    Rectangle {
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: 8
                                        height: 8
                                        radius: 1
                                        color: chip.tone.length > 0 ? Theme.roleName(chip.tone) : Theme.ink3
                                    }
                                    Text {
                                        textFormat: Text.PlainText
                                        text: chip.modelData
                                        color: Theme.ink1
                                        font: Theme.type.captionStrong
                                    }
                                }
                            }
                        }
                    }
                }

                // The servers and friends you share (8 of each, as the full
                // profile).
                Section {
                    visible: mutuals.any
                    ProfileMutuals {
                        id: mutuals
                        servers: root.loaded && !Profile.me ? Profile.mutualServers : ""
                        friends: root.loaded && !Profile.me ? Profile.mutualFriends : ""
                        limit: 8
                    }
                }

                Section {
                    title: root.since?.label ?? ""
                    visible: root.since !== null
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: root.since?.value ?? ""
                        color: Theme.ink1
                        font: Theme.type.description
                    }
                }
                // The board (ProfilePanel.tsx) on the desktop column's
                // ply-3, which the widgets' legends are cut from.
                Rectangle {
                    id: boardBand
                    readonly property int gutter: 16
                    width: parent.width
                    height: boardColumn.y + boardColumn.height + 24
                    visible: root.loaded && Profile.loadError.length === 0
                    color: Theme.ply3

                    Rectangle {
                        width: parent.width
                        height: 1
                        color: Theme.line
                    }

                    // Board · Activity · Wishlist.
                    Item {
                        id: boardHeader
                        width: parent.width
                        height: 48
                        Row {
                            id: boardTabs
                            // Each tab's 12 px padding sits outside the
                            // gutter, so its label lines up with the board.
                            x: boardBand.gutter - 12 + root.leftInset
                            height: parent.height
                            spacing: 4
                            Accessible.role: Accessible.PageTabList
                            Repeater {
                                model: board.tabs
                                delegate: Item {
                                    id: boardTab
                                    required property var modelData
                                    readonly property bool on: board.tab === modelData.id
                                    readonly property int count: modelData.id === "board" ? board.count : 0
                                    width: boardTabRow.implicitWidth + 24
                                    height: boardTabs.height
                                    Accessible.role: Accessible.PageTab
                                    Accessible.name: modelData.label + (count > 0 ? " " + count : "")
                                    Accessible.checked: on
                                    Accessible.onPressAction: board.chooseTab(modelData.id)
                                    Row {
                                        id: boardTabRow
                                        anchors.centerIn: parent
                                        spacing: 6
                                        Text {
                                            id: boardTabLabel
                                            textFormat: Text.PlainText
                                            text: boardTab.modelData.label
                                            color: boardTab.on ? Theme.ink1 : Theme.ink2
                                            font: Qt.font({ family: "Yap Bricolage Key Bold", pixelSize: 15, weight: Font.Bold })
                                        }
                                        Text {
                                            textFormat: Text.PlainText
                                            visible: boardTab.count > 0
                                            anchors.baseline: boardTabLabel.baseline
                                            text: String(boardTab.count)
                                            color: Theme.ink3
                                            font: Theme.type.num
                                        }
                                    }
                                    TapHandler {
                                        onTapped: {
                                            Native.haptic(Native.Tick)
                                            board.chooseTab(boardTab.modelData.id)
                                        }
                                    }
                                }
                            }
                        }
                        TabRule { row: boardTabs }
                        // Edit board at the tabs' end: the labelled key where
                        // it fits beside them, else the bare key.
                        Item {
                            id: editBoardSlot
                            readonly property bool can: board.shown && board.isOwner && board.tab === "board" && !board.loading && board.error.length === 0
                            readonly property real room: parent.width - boardTabs.x - boardTabs.width - boardBand.gutter - root.rightInset - 8
                            visible: can
                            anchors.right: parent.right
                            anchors.rightMargin: boardBand.gutter - 4 + root.rightInset
                            anchors.verticalCenter: parent.verticalCenter
                            width: editBoardWide.implicitWidth <= room ? editBoardWide.implicitWidth : 40
                            height: 40
                            KitButton {
                                id: editBoardWide
                                visible: implicitWidth <= editBoardSlot.room
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                size: "sm"
                                variant: "secondary"
                                ground: Theme.ply3
                                iconName: "edit"
                                text: qsTr("Edit board")
                                onClicked: root.editBoard()
                            }
                            IconKey {
                                visible: !editBoardWide.visible
                                anchors.fill: parent
                                iconName: "edit"
                                tip: qsTr("Edit board")
                                onClicked: root.editBoard()
                            }
                        }
                        Rectangle {
                            anchors.bottom: parent.bottom
                            width: parent.width
                            height: 1
                            color: Theme.line
                        }
                    }

                    Column {
                        id: boardColumn
                        x: boardBand.gutter + root.leftInset
                        y: boardHeader.height + 16
                        width: parent.width - 2 * boardBand.gutter - root.leftInset - root.rightInset
                        spacing: 16

                        // Loading: static plates in the Rack's shape.
                        Item {
                            visible: board.tab === "board" && (!board.shown || board.loading) && board.error.length === 0
                            width: parent.width
                            height: 8 + 2 * 151 + 20
                            Accessible.role: Accessible.StaticText
                            Accessible.name: qsTr("Loading board")
                            Repeater {
                                model: [[0, 0, 2], [0, 1, 1], [1, 1, 1]]
                                delegate: Rectangle {
                                    required property var modelData
                                    readonly property real colW: (boardColumn.width - 10) / 2
                                    x: modelData[0] * (colW + 10)
                                    y: 8 + modelData[1] * 171
                                    width: modelData[2] * colW + (modelData[2] - 1) * 10
                                    height: 151
                                    radius: Theme.radius
                                    color: Theme.tint1
                                    border.width: 1
                                    border.color: Theme.line
                                }
                            }
                        }
                        Column {
                            visible: board.shown && board.tab === "board" && board.error.length > 0
                            width: parent.width
                            spacing: 12
                            Text {
                                width: parent.width
                                textFormat: Text.PlainText
                                text: board.error
                                wrapMode: Text.Wrap
                                color: Theme.ink2
                                font: Theme.type.description
                            }
                            KitButton {
                                variant: "secondary"
                                iconName: "refresh"
                                text: qsTr("Try again")
                                onClicked: Board.retry()
                            }
                        }
                        Text {
                            visible: board.shown && board.tab === "board" && !board.loading && board.empty
                            width: parent.width
                            textFormat: Text.PlainText
                            text: board.isOwner
                                ? qsTr("Your board is empty. Add what you're playing, your mood, photos or a countdown with Edit board. Everyone who opens your profile sees it.")
                                : qsTr("%1 hasn't added anything to their board yet.").arg(Profile.name)
                            wrapMode: Text.Wrap
                            color: Theme.ink3
                            font: Theme.type.description
                        }

                        BoardRack {
                            visible: board.shown && board.tab === "board" && !board.loading && !board.empty
                            width: parent.width
                            rackJson: board.rack
                            boardData: board.boardData
                            ctx: board.ctx
                            boardState: board.st
                            editing: false
                        }
                        BoardActivity {
                            visible: board.shown && board.tab === "activity"
                            width: parent.width
                            view: board.activity
                            ownerName: Profile.name
                        }
                        BoardWishlist {
                            visible: board.shown && board.tab === "wishlist"
                            width: parent.width
                            view: board.wishlist
                            ownerName: Profile.name
                        }
                    }
                }
            }

            // Their profile effect over the page: one pass, then its still.
            Loader {
                x: body.x
                y: body.y
                width: body.width
                height: body.height
                active: root.loaded && Profile.effect.length > 0
                sourceComponent: ProfileEffectLayer {
                    effect: Profile.effect
                    playing: root.visible
                }
            }
            ProfileFrameLayer {
                visible: root.framed
                anchors.fill: parent
                frame: root.framed ? Profile.frame : ""
                order: "front"
            }
        }
    }

    // Close, over the banner: a plate so it reads on any picture.
    Rectangle {
        x: 12 + root.leftInset
        y: root.topInset + 8
        width: 40
        height: 40
        radius: Theme.radius
        color: Theme.ply0
        opacity: 0.92
        IconKey {
            anchors.fill: parent
            iconName: "chevronDown"
            tone: "rest"
            tip: qsTr("Close profile")
            onClicked: root.closeRequested()
        }
    }

    // A titled block of the sheet: a seam, the silkscreen label, the rest.
    component Section: Column {
        id: section
        property string title
        default property alias content: inner.data
        x: 20 + root.leftInset
        width: parent.width - 2 * x
        topPadding: 0
        bottomPadding: 16
        spacing: 8
        Rectangle {
            width: parent.width
            height: 1
            color: Theme.line
        }
        Text {
            visible: section.title.length > 0
            topPadding: 8
            text: section.title
            color: Theme.ink3
            font: Theme.type.silk
            Accessible.role: Accessible.Heading
        }
        Column {
            id: inner
            width: parent.width
        }
    }
}
