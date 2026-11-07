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
    property string localError
    property bool saved: false

    readonly property bool dirty: pronouns !== Settings.pronouns
        || bio !== Settings.bio
        || customStatus.trim() !== Settings.customStatus
        || accentColor !== Settings.accentColor
        || backgroundColor !== Settings.backgroundColor
        || bannerColor !== Settings.bannerColor
        || avatarPath.length > 0 || removeAvatar || avatarUrl.length > 0
        || bannerPath.length > 0 || removeBanner
    readonly property string error: localError.length > 0 ? localError : Settings.profileError
    // What the profile wears (collectibles apply at once, not on Save).
    readonly property var worn: Session.meDisplay.length > 0 ? (JSON.parse(Session.meDisplay).collectibles ?? {}) : ({})
    readonly property string shownAvatar: removeAvatar ? "" : avatarPath.length > 0 ? "file:///" + avatarPath
        : avatarUrlShown.length > 0 ? avatarUrlShown : Settings.avatar
    readonly property string shownBanner: removeBanner ? "" : bannerPath.length > 0 ? "file:///" + bannerPath : Settings.banner

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
        localError = ""
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
        }))
    }
    function requestClose() {
        if (dirty || Settings.profileSaving)
            bar.nudge()
        else
            close()
    }
    function pick(kind) {
        const paths = Files.pickFiles(kind === "avatar" ? qsTr("Choose an avatar") : qsTr("Choose a banner"))
        const path = paths.length > 0 ? paths[0] : ""
        if (path.length === 0)
            return
        if (Settings.imageType(path).length === 0) {
            localError = kind === "avatar" ? qsTr("Choose an image file for your avatar.") : qsTr("Choose an image file for your banner.")
            return
        }
        localError = ""
        if (kind === "avatar") {
            avatarPath = path
            removeAvatar = false
            avatarUrl = ""
            avatarUrlShown = ""
        } else {
            bannerPath = path
            removeBanner = false
        }
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
    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.dur4; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
            NumberAnimation { property: "scale"; from: Theme.reducedMotion ? 1 : 0.98; to: 1; duration: Theme.dur4; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
        }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: Theme.dur4Exit }
    }
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
                x: Math.max(24, (body.width - 1040) / 2)
                y: 24
                width: Math.min(1040, body.width - 48)
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
                    nameStyle: Session.meNameStyle
                    animate: editor.visible
                    name: Settings.name
                    username: Settings.username
                    avatar: editor.shownAvatar
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
                }
                // Your nameplate behind a 48 px member row: the avatar
                // (without its decoration) and the name at 15/500.
                Rectangle {
                    visible: Session.mePlate.length > 0
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
                        plate: Session.mePlate
                        playing: editor.visible
                        ground: Theme.ply2
                    }
                    Avatar {
                        id: plateAvatar
                        x: 12
                        anchors.verticalCenter: parent.verticalCenter
                        size: 32
                        initials: Settings.initials
                        source: editor.shownAvatar
                    }
                    Text {
                        anchors.left: plateAvatar.right
                        anchors.leftMargin: 12
                        anchors.right: parent.right
                        anchors.rightMargin: 12
                        anchors.verticalCenter: parent.verticalCenter
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                        text: Settings.name
                        color: Theme.ink1
                        font: Theme.type.setting
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
                                : editor.avatarUrl.length > 0 ? qsTr("A recent avatar, used when you save")
                                : Settings.customAvatar ? qsTr("Your current avatar")
                                : qsTr("Using your sign-in picture")
                            leading: Component {
                                Thumb {
                                    Avatar {
                                        size: 44
                                        initials: Settings.name
                                        source: editor.shownAvatar
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
                                KitButton {
                                    visible: editor.avatarPath.length > 0 || editor.avatarUrl.length > 0
                                        || (Settings.customAvatar && !editor.removeAvatar)
                                    variant: "ghost"
                                    text: qsTr("Remove")
                                    Accessible.name: qsTr("Remove avatar")
                                    onClicked: {
                                        editor.localError = ""
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
                            layout: "inline"
                            ComingLater {}
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
                            custom: true; rowId: "profile-decoration"; label: qsTr("Avatar decoration"); layout: "inline"
                            description: editor.worn.avatarDecoration?.name ?? qsTr("None")
                            CollectibleKeys { kind: "avatar_decoration"; on: editor.worn.avatarDecoration !== undefined }
                        }
                        SettingsRow {
                            custom: true; rowId: "profile-effect"; label: qsTr("Profile effect"); layout: "inline"
                            description: editor.worn.profileEffect?.name ?? qsTr("None")
                            CollectibleKeys { kind: "profile_effect"; on: editor.worn.profileEffect !== undefined }
                        }
                        SettingsRow {
                            custom: true; rowId: "profile-frame"; label: qsTr("Profile frame"); layout: "inline"
                            description: editor.worn.profileFrame?.name ?? qsTr("None")
                            CollectibleKeys { kind: "profile_frame"; on: editor.worn.profileFrame !== undefined }
                        }
                        SettingsRow {
                            custom: true; rowId: "profile-nameplate"; label: qsTr("Nameplate"); layout: "inline"
                            description: editor.worn.nameplate?.name ?? qsTr("None")
                            CollectibleKeys { kind: "nameplate"; on: editor.worn.nameplate !== undefined }
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
        width: wide ? 80 : 44
        height: 44
        radius: Theme.radius
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
            size: "sm"
            variant: "secondary"
            text: qsTr("Change")
            Accessible.name: qsTr("Change %1").arg(keys.label)
            onClicked: picker.show(keys.kind)
        }
        KitButton {
            visible: keys.on
            size: "sm"
            variant: "ghost"
            text: qsTr("Remove")
            Accessible.name: qsTr("Remove %1").arg(keys.label)
            onClicked: Shop.remove(keys.kind)
        }
    }

    CollectiblePicker {
        id: picker
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
        }
    }
}
