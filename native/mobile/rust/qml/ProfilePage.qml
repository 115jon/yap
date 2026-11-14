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
// still (only the still under reduced motion). Video banners and the
// profile board are the desktop's only.
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
    }
    Component.onDestruction: Profile.close(userId)

    Flickable {
        id: scroll
        anchors.fill: parent
        contentHeight: framedBox.y + framedBox.height + root.bottomInset + 24
        boundsBehavior: Flickable.StopAtBounds
        flickDeceleration: Native.flingDeceleration
        maximumFlickVelocity: Native.maxFlingVelocity

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
                        source: root.loaded ? Profile.banner : ""
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                        sourceSize.width: 1080
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
