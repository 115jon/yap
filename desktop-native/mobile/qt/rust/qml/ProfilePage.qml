pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Someone's profile, a full-height sheet over the pager, on the desktop's
// Profile object (show, close): the banner, the avatar with presence, the
// name and handle, their status, Message (Edit profile on your own), then
// About, roles and Member since. Opened from a message's avatar or name, a
// member row, a DM row's avatar or a DM's header.
//
// TODO: present it in the shared BottomSheet (drag to dismiss) once that
// is on main; until then it is a page of the stack that rises from the
// bottom edge. The desktop's ProfileCard (effects, frames, video banners,
// badges, mutuals) needs QtMultimedia and desktop-only layers, so this is a
// phone layout of the same fields.
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
    signal editRequested()

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

    color: Theme.ply1
    transform: Translate { y: root.backProgress * root.height * 0.12 }

    Component.onCompleted: Profile.show(userId)
    Component.onDestruction: Profile.close(userId)

    Flickable {
        id: scroll
        anchors.fill: parent
        contentHeight: body.height + root.bottomInset + 24
        boundsBehavior: Flickable.StopAtBounds
        flickDeceleration: Native.flingDeceleration
        maximumFlickVelocity: Native.maxFlingVelocity

        Column {
            id: body
            width: scroll.width

            Rectangle {
                id: bannerArea
                width: parent.width
                height: root.topInset + root.bannerHeight
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
                    onClicked: root.editRequested()
                }
            }
            Item {
                width: parent.width
                height: messageKey.height + 16
                visible: root.loaded && !Profile.me
                KitButton {
                    id: messageKey
                    x: 20 + root.leftInset
                    width: parent.width - 2 * x
                    variant: "primary"
                    ground: root.color
                    iconName: "chat"
                    text: qsTr("Message")
                    onClicked: {
                        Native.haptic(Native.Confirm)
                        root.messageRequested(root.userId)
                    }
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
