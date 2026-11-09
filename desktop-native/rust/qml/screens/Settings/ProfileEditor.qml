pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import Yap.Chrome
import com.yap.native

// The profile editor (ProfileEditorModal.tsx, SettingsAccountTab.tsx): the
// workbench frame at min(1120, window - 32) over the stacked scrim. A 56 px
// header ("Edit profile", "Changes saved" for a moment after a save, the
// close key), then one scroller: the live preview card (340 wide) beside
// the form once it fits, above it otherwise. The form: Avatar and banner,
// Profile theme, About you. Edits are a draft until Save in the unsaved
// bar; closing while dirty nudges the bar instead of dropping the draft.
// A new avatar is framed first (AvatarCropDialog: the crop is saved beside
// the picture), Crop frames the current one again; the name style opens
// NameStyleDialog; Upload takes a picture or video as the nameplate.
T.Popup {
    id: editor

    // The draft.
    property string pronouns
    property string bio
    property string customStatus
    property string accentColor
    property string backgroundColor
    property string bannerColor
    property string avatarPath
    property bool removeAvatar: false
    // A recent avatar picked again (AvatarPickerModal.tsx): the stored URL
    // the save sends, and its picture.
    property string avatarUrl
    property string avatarUrlShown
    property string bannerPath
    property bool removeBanner: false
    // The frame editor's crop (`{x, y, width, height}` in percent, or
    // null), when it changed (a recent avatar starts without one).
    property var crop: null
    property bool cropChanged: false
    // The display-name style as the views carry it, or empty.
    property string nameStyle
    // A picture or video for the nameplate, or the upload to delete.
    property string nameplatePath
    property bool removeNameplate: false
    property string localError
    // A banner over its limit (the too-big card under the banner row).
    property string bannerTooBig
    property bool saved: false

    readonly property bool dirty: pronouns !== Settings.pronouns
        || bio !== Settings.bio
        || customStatus.trim() !== Settings.customStatus
        || accentColor !== Settings.accentColor
        || backgroundColor !== Settings.backgroundColor
        || bannerColor !== Settings.bannerColor
        || avatarPath.length > 0 || removeAvatar || avatarUrl.length > 0
        || bannerPath.length > 0 || removeBanner
        || cropChanged || nameStyle !== Session.meNameStyle
        || nameplatePath.length > 0 || removeNameplate
    readonly property string error: localError.length > 0 ? localError : Settings.profileError
    // What the profile wears (collectibles apply at once, not on Save; a
    // removed nameplate waits for Save, removePlate).
    readonly property var worn: Session.meDisplay.length > 0 ? (JSON.parse(Session.meDisplay).collectibles ?? {}) : ({})
    readonly property string shownAvatar: removeAvatar ? "" : avatarPath.length > 0 ? "file:///" + avatarPath
        : avatarUrlShown.length > 0 ? avatarUrlShown : Settings.avatar
    readonly property string shownBanner: removeBanner ? "" : bannerPath.length > 0 ? "file:///" + bannerPath : Settings.banner
    // The stored crop, then the draft's (avatarDisplayToImageStyle).
    readonly property var storedCrop: {
        if (Session.meDisplay.length === 0)
            return null
        const display = JSON.parse(Session.meDisplay)
        return display && display.crop ? display.crop : null
    }
    readonly property var shownCrop: removeAvatar ? null : cropChanged ? crop : storedCrop
    readonly property string nameplateType: nameplatePath.length > 0 ? Settings.nameplateType(nameplatePath) : ""
    // The plate the preview wears: the picked file, else the saved one.
    readonly property string shownPlate: removeNameplate ? ""
        : nameplatePath.length > 0 ? JSON.stringify({
            still: nameplateType.startsWith("video/") ? "" : "file:///" + nameplatePath,
            motion: nameplateType.startsWith("video/") ? "file:///" + nameplatePath : "",
            border: "", light: false, assist: true
        })
        : Session.mePlate

    function reset() {
        pronouns = Settings.pronouns
        bio = Settings.bio
        customStatus = Settings.customStatus
        accentColor = Settings.accentColor
        backgroundColor = Settings.backgroundColor
        bannerColor = Settings.bannerColor
        avatarPath = ""
        removeAvatar = false
        avatarUrl = ""
        avatarUrlShown = ""
        bannerPath = ""
        removeBanner = false
        crop = null
        cropChanged = false
        nameStyle = Session.meNameStyle
        nameplatePath = ""
        removeNameplate = false
        localError = ""
        bannerTooBig = ""
        saved = false
    }
    function save() {
        localError = ""
        Settings.saveProfile(JSON.stringify({
            pronouns: pronouns,
            bio: bio,
            customStatus: customStatus,
            accentColor: accentColor,
            backgroundColor: backgroundColor,
            bannerColor: bannerColor,
            avatarPath: avatarPath,
            removeAvatar: removeAvatar,
            avatarUrl: avatarUrl,
            bannerPath: bannerPath,
            removeBanner: removeBanner,
            crop: crop,
            cropChanged: cropChanged,
            nameStyle: nameStyle,
            nameStyleChanged: nameStyle !== Session.meNameStyle,
            nameplatePath: nameplatePath,
            removeNameplate: removeNameplate,
        }))
    }
    function requestClose() {
        if (dirty || Settings.profileSaving)
            bar.nudge()
        else
            close()
    }
    // The avatar's file comes from Choose an image, which stays open under
    // the system dialog: a file over the limit shows its too-big card there
    // (Pick another asks again); one that fits closes it for the crop. A
    // banner over the limit shows its card under the banner row. Nothing
    // over its limit joins the draft, so nothing over it is uploaded.
    function pick(kind) {
        const paths = Files.pickFiles(kind === "avatar" ? qsTr("Choose an avatar") : qsTr("Choose a banner"))
        const path = paths.length > 0 ? paths[0] : ""
        if (path.length === 0)
            return
        if (Settings.imageType(path).length === 0) {
            avatarPicker.close()
            localError = kind === "avatar" ? qsTr("Choose an image file for your avatar.") : qsTr("Choose an image file for your banner.")
            return
        }
        const tooBig = Limits.check(kind, path)
        if (kind === "avatar") {
            avatarPicker.tooBig = tooBig
            if (tooBig.length > 0)
                return
            avatarPicker.close()
        } else {
            bannerTooBig = tooBig
            if (tooBig.length > 0)
                return
        }
        localError = ""
        if (kind === "avatar") {
            // handleAvatarFile: framed before it joins the draft.
            cropper.filePath = path
            cropper.source = "file:///" + path
            cropper.open()
        } else {
            bannerPath = path
            removeBanner = false
        }
    }

    // Upload a nameplate (handleNameplateSelect): a picture or a video.
    function pickNameplate() {
        const paths = Files.pickFiles(qsTr("Upload a nameplate"))
        const path = paths.length > 0 ? paths[0] : ""
        if (path.length === 0)
            return
        if (Settings.nameplateType(path).length === 0) {
            localError = qsTr("Choose an image or video file for your nameplate.")
            return
        }
        localError = ""
        removeNameplate = false
        nameplatePath = path
    }
    // handleRemoveNameplate: a picked file is dropped, and the saved plate,
    // uploaded or collectible, comes off on Save (the draft; the core
    // deletes the upload and empties the slot).
    function removePlate() {
        localError = ""
        nameplatePath = ""
        if (Session.mePlate.length > 0 || worn.nameplate !== undefined)
            removeNameplate = true
    }

    parent: T.Overlay.overlay
    // Centred under the title bar, which the web keeps above overlays.
    x: Math.round((parent.width - width) / 2)
    y: Theme.chrome + Math.round((parent.height - Theme.chrome - height) / 2)
    width: Math.min(1120, parent.width - 32)
    height: Math.min(820, parent.height - Theme.chrome - 32)
    padding: 0
    modal: true
    // Ctrl+K stays shut while a modal is up (ExtrasHost).
    onVisibleChanged: ShellState.modalCount += visible ? 1 : -1
    focus: true
    closePolicy: T.Popup.NoAutoClose
    onAboutToShow: reset()
    // Opened over Settings, the focus stays in Settings unless the editor
    // takes it: Escape (handled by the content scope) would never reach it.
    onOpened: contentItem.forceActiveFocus()

    T.Overlay.modal: Scrim {
        color: Theme.scrimStacked
        stepsBack: true
        onTapped: editor.requestClose()
    }
    enter: PresenceIn { duration: Theme.dur4; fromScale: Theme.scaleDialogOut }
    exit: PresenceOut { duration: Theme.dur4Exit; toScale: Theme.scaleDialogOut }
    background: Rectangle {
        radius: Theme.radiusSheet
        color: Theme.ply0
        border.width: 1
        border.color: Theme.line
    }

    // A save that went through: the draft is the profile now.
    Connections {
        target: Settings
        function onProfileSavedChanged() {
            if (!editor.visible)
                return
            editor.reset()
            editor.saved = true
            savedTimer.restart()
        }
    }
    Timer {
        id: savedTimer
        interval: 2400
        onTriggered: editor.saved = false
    }

    contentItem: FocusScope {
        focus: true
        Keys.onEscapePressed: event => {
            event.accepted = true
            editor.requestClose()
        }

        Item {
            id: header
            width: parent.width
            height: 56
            Text {
                x: 24
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Edit profile")
                color: Theme.ink1
                font: Theme.type.profileName
                Accessible.role: Accessible.Heading
            }
            Row {
                visible: editor.saved && !editor.dirty
                anchors.right: closeKey.left
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 6
                Accessible.role: Accessible.StaticText
                Accessible.name: qsTr("Changes saved")
                Icon {
                    anchors.verticalCenter: parent.verticalCenter
                    name: "check"
                    size: Theme.iconSm
                    color: Theme.successInk
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Changes saved")
                    color: Theme.ink2
                    font: Theme.type.description
                }
            }
            IconKey {
                id: closeKey
                anchors.right: parent.right
                anchors.rightMargin: 16
                anchors.verticalCenter: parent.verticalCenter
                size: 28
                iconName: "close"
                tip: qsTr("Close profile editor")
                tipSide: "bottom"
                onClicked: editor.requestClose()
            }
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 1
                color: Theme.line
            }
        }

        Flickable {
            id: body
            anchors.top: header.bottom
            anchors.bottom: bar.top
            width: parent.width
            clip: true
            contentHeight: layout.height + 48
            boundsBehavior: Flickable.StopAtBounds

            Item {
                id: layout
                readonly property bool beside: width >= 840
                // max-w-[1040px] holds the md:px-6 padding: the content is
                // 48 narrower than the box, centred.
                x: Math.round((body.width - Math.min(1040, body.width)) / 2) + 24
                y: 24
                width: Math.min(1040, body.width) - 48
                height: beside ? Math.max(previewStack.height, form.height) : previewStack.height + 24 + form.height

                // The popover card others open, from the draft, wearing your
                // collectibles; under it, your nameplate behind a member row
                // (SettingsAccountTab.tsx's preview section).
                Column {
                    id: previewStack
                    x: layout.beside ? 0 : (layout.width - width) / 2
                    y: layout.beside ? Math.max(0, Math.min(body.contentY, form.height - height)) : 0
                    width: 340
                    spacing: 16
                ProfileCard {
                    id: preview
                    width: 340
                    deco: editor.shownAvatar.length > 0 ? Session.meDeco : ""
                    effect: Session.meEffect
                    frame: Session.meFrame
                    nameStyle: editor.nameStyle
                    badges: Settings.badges
                    animate: editor.visible
                    name: Settings.name
                    username: Settings.username
                    avatar: editor.shownAvatar
                    crop: editor.shownCrop
                    initials: Settings.initials
                    pronouns: editor.pronouns.trim()
                    presence: Settings.status
                    customStatus: editor.customStatus.trim()
                    banner: editor.shownBanner
                    bannerColor: editor.bannerColor
                    surfaceColor: editor.backgroundColor
                    accentColor: editor.accentColor
                    Accessible.role: Accessible.Graphic
                    Accessible.name: qsTr("Profile preview")
                    Column {
                        visible: editor.bio.trim().length > 0
                        width: parent.width
                        spacing: 8
                        Text {
                            text: qsTr("About me")
                            color: preview.subInk
                            font: Theme.type.captionStrong
                        }
                        Text {
                            width: parent.width
                            textFormat: Text.PlainText
                            text: editor.bio.trim()
                            wrapMode: Text.Wrap
                            color: preview.ink
                            font: Theme.type.description
                        }
                    }
                    // ProfileSections `memberSince`: "Member Since" this
                    // server, else "Account Created".
                    Column {
                        readonly property var since: Settings.memberSince.length > 0 ? JSON.parse(Settings.memberSince) : null
                        visible: since !== null
                        width: parent.width
                        spacing: 8
                        Text {
                            textFormat: Text.PlainText
                            text: parent.since ? parent.since.label : ""
                            color: preview.subInk
                            font: Theme.type.captionStrong
                            Accessible.role: Accessible.Heading
                        }
                        Text {
                            textFormat: Text.PlainText
                            text: parent.since ? parent.since.value : ""
                            color: preview.ink
                            font: Theme.type.description
                        }
                    }
                }
                // Your nameplate behind a 48 px member row: the avatar
                // (without its decoration) and the name at 15/500.
                Rectangle {
                    visible: editor.shownPlate.length > 0
                    width: parent.width
                    height: 48
                    radius: 8
                    color: Theme.ply2
                    border.width: 1
                    border.color: Theme.line
                    clip: true
                    Accessible.role: Accessible.Graphic
                    Accessible.name: qsTr("Nameplate preview")
                    NameplateLayer {
                        anchors.fill: parent
                        anchors.margins: 1
                        plate: editor.shownPlate
                        playing: editor.visible
                        // A custom plate fades in from the left
                        // (UserNameplateLayer.tsx nameplateIdentity mask).
                        identityFade: true
                        ground: Theme.ply2
                    }
                    Avatar {
                        id: plateAvatar
                        x: 12
                        anchors.verticalCenter: parent.verticalCenter
                        size: 32
                        initials: Settings.initials
                        source: editor.shownAvatar
                        crop: editor.shownCrop
                    }
                    // The name in its style, held to 2.8:1 on the plate.
                    DisplayName {
                        anchors.left: plateAvatar.right
                        anchors.leftMargin: 12
                        anchors.verticalCenter: parent.verticalCenter
                        width: Math.min(implicitWidth, parent.width - plateAvatar.width - 36)
                        text: Settings.name
                        nameStyle: editor.nameStyle
                        color: Theme.ink1
                        font: Theme.type.setting
                        ground: Theme.ply2
                        minContrast: 2.8
                    }
                }
                }

                Column {
                    id: form
                    x: layout.beside ? 340 + 32 : 0
                    y: layout.beside ? 0 : previewStack.height + 24
                    width: layout.beside ? layout.width - 340 - 32 : layout.width
                    spacing: 24

                    SettingsCard {
                        width: parent.width
                        title: qsTr("Avatar and banner")
                        SettingsRow {
                            custom: true
                            rowId: "profile-avatar"
                            label: qsTr("Avatar")
                            description: editor.removeAvatar ? qsTr("Removed when you save")
                                : editor.avatarPath.length > 0 ? qsTr("New image, uploads when you save")
                                : editor.avatarUrl.length > 0 ? qsTr("Recent image, applies when you save")
                                : Settings.customAvatar ? qsTr("Your current avatar")
                                : qsTr("Using your sign-in picture")
                            // Thumb round: the picture cropped round, or the
                            // name's initial at 15/600 on the tint.
                            leading: Component {
                                Thumb {
                                    round: true
                                    Avatar {
                                        visible: editor.shownAvatar.length > 0
                                        anchors.centerIn: parent
                                        size: 42
                                        round: true
                                        initials: Settings.name
                                        source: editor.shownAvatar
                                        crop: editor.shownCrop
                                    }
                                    Text {
                                        visible: editor.shownAvatar.length === 0
                                        anchors.centerIn: parent
                                        textFormat: Text.PlainText
                                        text: Settings.name.trim().charAt(0).toUpperCase()
                                        color: Theme.ink1
                                        font.family: "Yap Geist SemiBold"
                                        font.pixelSize: 15
                                        font.weight: Font.DemiBold
                                    }
                                }
                            }
                            Row {
                                spacing: 8
                                KitButton {
                                    text: qsTr("Change")
                                    Accessible.name: qsTr("Change avatar")
                                    onClicked: avatarPicker.open()
                                }
                                // The current picture framed again.
                                KitButton {
                                    variant: "ghost"
                                    text: qsTr("Crop")
                                    Accessible.name: qsTr("Crop avatar")
                                    enabled: editor.shownAvatar.length > 0
                                    onClicked: {
                                        cropper.filePath = ""
                                        cropper.source = editor.shownAvatar
                                        cropper.open()
                                    }
                                }
                                KitButton {
                                    visible: editor.avatarPath.length > 0 || editor.avatarUrl.length > 0
                                        || (!editor.removeAvatar && (Settings.customAvatar || editor.shownCrop !== null))
                                    variant: "ghost"
                                    text: qsTr("Remove")
                                    Accessible.name: qsTr("Remove avatar")
                                    onClicked: {
                                        editor.localError = ""
                                        editor.crop = null
                                        editor.cropChanged = false
                                        if (editor.avatarPath.length > 0 || editor.avatarUrl.length > 0) {
                                            editor.avatarPath = ""
                                            editor.avatarUrl = ""
                                            editor.avatarUrlShown = ""
                                        } else {
                                            editor.removeAvatar = true
                                        }
                                    }
                                }
                            }
                        }
                        SettingsRow {
                            custom: true
                            rowId: "profile-banner"
                            label: qsTr("Banner")
                            description: editor.removeBanner ? qsTr("Removed when you save")
                                : editor.bannerPath.length > 0 ? qsTr("New image, uploads when you save")
                                : Settings.banner.length > 0 ? qsTr("Your current banner")
                                : qsTr("No banner. Your banner color shows instead.")
                            leading: Component {
                                Thumb {
                                    wide: true
                                    color: editor.bannerColor.length > 0 ? editor.bannerColor
                                        : editor.accentColor.length > 0 ? editor.accentColor : Theme.accentTint
                                    Image {
                                        anchors.fill: parent
                                        visible: editor.shownBanner.length > 0
                                        source: editor.shownBanner
                                        fillMode: Image.PreserveAspectCrop
                                        asynchronous: true
                                        sourceSize.width: 160
                                    }
                                }
                            }
                            Row {
                                spacing: 8
                                KitButton {
                                    text: qsTr("Change")
                                    Accessible.name: qsTr("Change banner")
                                    onClicked: editor.pick("banner")
                                }
                                KitButton {
                                    visible: editor.shownBanner.length > 0
                                    variant: "ghost"
                                    text: qsTr("Remove")
                                    Accessible.name: qsTr("Remove banner")
                                    onClicked: {
                                        if (editor.bannerPath.length > 0)
                                            editor.bannerPath = ""
                                        else
                                            editor.removeBanner = true
                                    }
                                }
                            }
                        }
                        Item {
                            visible: editor.bannerTooBig.length > 0
                            width: parent.width
                            height: visible ? bannerCard.height + 16 : 0
                            TooBigCard {
                                id: bannerCard
                                y: 4
                                width: parent.width
                                info: editor.bannerTooBig.length > 0 ? JSON.parse(editor.bannerTooBig) : ({})
                                onAction: editor.pick("banner")
                            }
                        }
                        SettingsRow {
                            custom: true
                            rowId: "profile-banner-color"
                            label: qsTr("Banner color")
                            description: qsTr("Shows when you have no banner image.")
                            layout: "block"
                            ColorControl {
                                width: parent.width
                                name: qsTr("banner color")
                                value: editor.bannerColor
                                onPicked: value => editor.bannerColor = value
                            }
                        }
                    }

                    SettingsCard {
                        width: parent.width
                        title: qsTr("Profile theme")
                        description: qsTr("Colors for your profile card. Leave them on Auto to follow your app theme.")
                        SettingsRow {
                            custom: true
                            rowId: "profile-background"
                            label: qsTr("Background")
                            layout: "block"
                            ColorControl {
                                width: parent.width
                                name: qsTr("background color")
                                value: editor.backgroundColor
                                onPicked: value => editor.backgroundColor = value
                            }
                        }
                        SettingsRow {
                            custom: true
                            rowId: "profile-accent"
                            label: qsTr("Accent")
                            description: qsTr("Mixed into the background and used for highlights.")
                            layout: "block"
                            ColorControl {
                                width: parent.width
                                name: qsTr("accent color")
                                value: editor.accentColor
                                onPicked: value => editor.accentColor = value
                            }
                        }
                    }

                    SettingsCard {
                        width: parent.width
                        title: qsTr("About you")
                        SettingsRow {
                            custom: true
                            rowId: "profile-name-style"
                            label: qsTr("Display name style")
                            description: Settings.name
                            // The name in the draft's style, 16/600.
                            descriptionItem: Component {
                                DisplayName {
                                    text: Settings.name
                                    nameStyle: editor.nameStyle
                                    color: Theme.ink2
                                    font: Theme.type.menuTitle
                                    ground: Theme.ply2
                                }
                            }
                            KitButton {
                                text: editor.nameStyle.length > 0 ? qsTr("Edit") : qsTr("Add")
                                Accessible.name: editor.nameStyle.length > 0 ? qsTr("Edit display name style") : qsTr("Add display name style")
                                onClicked: styleDialog.open()
                            }
                            KitButton {
                                visible: editor.nameStyle.length > 0
                                variant: "ghost"
                                text: qsTr("Clear")
                                Accessible.name: qsTr("Clear display name style")
                                onClicked: editor.nameStyle = ""
                            }
                        }
                        SettingsRow {
                            custom: true
                            rowId: "profile-custom-status"
                            label: qsTr("Custom status")
                            description: qsTr("Shown under your name.")
                            layout: "block"
                            CountedField {
                                width: parent.width
                                accessibleName: qsTr("Custom status")
                                max: 128
                                placeholder: qsTr("Shipping the release")
                                value: editor.customStatus
                                onEdited: value => editor.customStatus = value
                            }
                        }
                        SettingsRow {
                            custom: true
                            rowId: "profile-pronouns"
                            label: qsTr("Pronouns")
                            layout: "block"
                            CountedField {
                                width: parent.width
                                accessibleName: qsTr("Pronouns")
                                max: 40
                                placeholder: qsTr("they/them")
                                value: editor.pronouns
                                onEdited: value => editor.pronouns = value
                            }
                        }
                        SettingsRow {
                            custom: true
                            rowId: "profile-bio"
                            label: qsTr("About me")
                            layout: "block"
                            CountedField {
                                width: parent.width
                                accessibleName: qsTr("About me")
                                max: 190
                                multiline: true
                                placeholder: qsTr("Night owl, synth collector, always up for co-op.")
                                value: editor.bio
                                onEdited: value => editor.bio = value
                            }
                        }
                    }

                    SettingsCard {
                        width: parent.width
                        title: qsTr("Collectibles")
                        description: qsTr("Decorations from your collection.")
                        SettingsRow {
                            custom: true; rowId: "profile-decoration"; label: qsTr("Avatar decoration")
                            description: editor.worn.avatarDecoration?.name ?? qsTr("None")
                            // The worn ring art at 36, or the sparkle.
                            leading: Component {
                                Thumb {
                                    Image {
                                        visible: Session.meDeco.length > 0
                                        anchors.centerIn: parent
                                        width: 36
                                        height: 36
                                        source: Session.meDeco
                                        sourceSize: Qt.size(72, 72)
                                        fillMode: Image.PreserveAspectFit
                                        asynchronous: true
                                    }
                                    Icon {
                                        visible: Session.meDeco.length === 0
                                        anchors.centerIn: parent
                                        name: "sparkle"
                                        color: Theme.ink3
                                    }
                                }
                            }
                            CollectibleKeys { kind: "avatar_decoration"; on: editor.worn.avatarDecoration !== undefined }
                        }
                        SettingsRow {
                            custom: true; rowId: "profile-effect"; label: qsTr("Profile effect")
                            description: editor.worn.profileEffect?.name ?? qsTr("None")
                            CollectibleKeys { kind: "profile_effect"; on: editor.worn.profileEffect !== undefined }
                        }
                        SettingsRow {
                            custom: true; rowId: "profile-frame"; label: qsTr("Profile frame")
                            description: editor.worn.profileFrame?.name ?? qsTr("None")
                            CollectibleKeys { kind: "profile_frame"; on: editor.worn.profileFrame !== undefined }
                        }
                        SettingsRow {
                            custom: true; rowId: "profile-nameplate"; label: qsTr("Nameplate")
                            description: editor.removeNameplate ? qsTr("Removed when you save")
                                : editor.nameplatePath.length > 0 ? qsTr("New file, uploads when you save")
                                : editor.worn.nameplate?.name ?? (Session.mePlate.length > 0 ? qsTr("Custom nameplate") : qsTr("None"))
                            Row {
                                spacing: 8
                                KitButton {
                                    variant: "secondary"
                                    text: qsTr("Change")
                                    Accessible.name: qsTr("Change nameplate")
                                    onClicked: picker.show("nameplate")
                                }
                                KitButton {
                                    variant: "ghost"
                                    text: qsTr("Upload")
                                    Accessible.name: qsTr("Upload a nameplate")
                                    onClicked: editor.pickNameplate()
                                }
                                KitButton {
                                    visible: editor.nameplatePath.length > 0
                                        || (!editor.removeNameplate && (Session.mePlate.length > 0 || editor.worn.nameplate !== undefined))
                                    variant: "ghost"
                                    text: qsTr("Remove")
                                    Accessible.name: qsTr("Remove nameplate")
                                    onClicked: editor.removePlate()
                                }
                            }
                        }
                    }
                }
            }
        }

        UnsavedBar {
            id: bar
            anchors.bottom: parent.bottom
            dirty: editor.dirty || editor.error.length > 0
            saving: Settings.profileSaving
            error: editor.error
            onReset: editor.reset()
            onSave: editor.save()
        }
    }

    // A small preview tile in a row's leading slot: 44 high, square (or
    // 80 wide), on tint-1 with a line edge.
    component Thumb: Rectangle {
        property bool wide: false
        property bool round: false
        width: wide ? 80 : 44
        height: 44
        radius: round ? 22 : Theme.radius
        color: Theme.tint1
        border.width: 1
        border.color: Theme.line
        clip: true
    }

    // A text field with its count ("12/40") under it, one line or three.
    component CountedField: Column {
        id: counted
        property string value
        property string placeholder
        property string accessibleName
        property int max: 40
        property bool multiline: false
        signal edited(string value)
        spacing: 4
        KitField {
            visible: !counted.multiline
            width: parent.width
            text: counted.value
            maximumLength: counted.max
            placeholderText: counted.placeholder
            Accessible.name: counted.accessibleName
            onTextEdited: counted.edited(text)
        }
        Rectangle {
            visible: counted.multiline
            width: parent.width
            height: 80
            radius: Theme.radius
            color: Theme.ply2
            border.width: 1
            border.color: area.activeFocus ? Theme.ink1 : areaHover.hovered ? Theme.ink2 : Theme.lineField
            EaseColor on border.color {}
            // styles.css :focus-visible: a focused input wears the 2 px --focus
            // outline outside its 1 px ink-1 edge (offset 0).
            Rectangle {
                anchors.fill: parent
                anchors.margins: -2
                visible: area.activeFocus
                radius: Theme.radius + 2
                color: "transparent"
                border.width: 2
                border.color: Theme.focus
            }
            HoverHandler { id: areaHover }
            TextEdit {
                id: area
                x: 12
                y: 8
                width: parent.width - 24
                height: parent.height - 16
                wrapMode: TextEdit.Wrap
                color: Theme.ink1
                selectionColor: Theme.selection
                selectedTextColor: Theme.ink1
                font: Theme.type.description
                selectByMouse: true
                Accessible.name: counted.accessibleName
                // Follows the draft without a binding: the clamp below
                // assigns `text`.
                Component.onCompleted: text = counted.value
                Connections {
                    target: counted
                    function onValueChanged() {
                        if (area.text !== counted.value)
                            area.text = counted.value
                    }
                }
                onTextChanged: {
                    if (text.length > counted.max) {
                        const at = cursorPosition
                        text = text.slice(0, counted.max)
                        cursorPosition = Math.min(at, counted.max)
                    }
                    if (text !== counted.value)
                        counted.edited(text)
                }
                Text {
                    visible: area.text.length === 0
                    textFormat: Text.PlainText
                    text: counted.placeholder
                    color: Theme.ink3
                    font: Theme.type.description
                }
            }
        }
        Text {
            anchors.right: parent.right
            textFormat: Text.PlainText
            text: counted.value.length + "/" + counted.max
            color: Theme.ink3
            font: Theme.type.numSm
        }
    }

    // A collectible row's keys: Change opens the slot's picker; Remove
    // (while the slot holds one) empties it at once.
    component CollectibleKeys: Row {
        id: keys
        required property string kind
        property bool on: false
        readonly property string label: kind === "avatar_decoration" ? qsTr("avatar decoration")
            : kind === "profile_effect" ? qsTr("profile effect")
            : kind === "profile_frame" ? qsTr("profile frame") : qsTr("nameplate")
        spacing: 8
        KitButton {
            variant: "secondary"
            text: qsTr("Change")
            Accessible.name: qsTr("Change %1").arg(keys.label)
            onClicked: picker.show(keys.kind)
        }
        KitButton {
            visible: keys.on
            variant: "ghost"
            text: qsTr("Remove")
            Accessible.name: qsTr("Remove %1").arg(keys.label)
            onClicked: Shop.remove(keys.kind)
        }
    }

    CollectiblePicker {
        id: picker
    }

    // Frame your avatar: a picked file (Apply puts it in the draft) or the
    // current picture again (Apply changes the crop only).
    AvatarCropDialog {
        id: cropper
        property string filePath
        name: Settings.name
        initialCrop: editor.shownCrop
        round: Session.meDeco.length > 0
        onApplied: crop => {
            if (filePath.length > 0) {
                editor.avatarPath = filePath
                editor.avatarUrl = ""
                editor.avatarUrlShown = ""
            }
            editor.removeAvatar = false
            editor.crop = crop
            editor.cropChanged = true
        }
    }

    NameStyleDialog {
        id: styleDialog
        displayName: Settings.name
        username: Settings.username
        avatar: editor.shownAvatar
        avatarCrop: editor.shownCrop
        initials: Settings.initials
        initialStyle: editor.nameStyle
        surfaceColor: editor.backgroundColor
        accentColor: editor.accentColor
        bannerColor: editor.bannerColor
        // INTEGRATION POINT (native/tiers): bind the signed-in user's
        // `allNameStyles` perk here, as the web does
        // (hasPerk(entitlement, "allNameStyles")). Everyone on production
        // is a founder, and update-profile refuses a Yap style without it.
        canUseYapStyles: true
        onApplied: style => editor.nameStyle = style
    }

    // Choose an image (AvatarPickerModal.tsx): upload, or one of the last
    // six uploads again.
    AvatarPicker {
        id: avatarPicker
        name: Settings.name
        onUploadRequested: editor.pick("avatar")
        onRecentPicked: (stored, url) => {
            editor.localError = ""
            editor.avatarPath = ""
            editor.removeAvatar = false
            editor.avatarUrl = stored
            editor.avatarUrlShown = url
            // withoutCrop: a recent picture starts unframed.
            editor.crop = null
            editor.cropChanged = true
        }
    }
}
