import QtQuick
import Yap.Ui

// The profile editor's draft as both apps edit it (ProfileEditorModal.tsx,
// SettingsAccountTab.tsx): the desktop's ProfileEditor and the phone's
// Edit profile page draw it their own way and keep only their pickers.
// Edits stay here until save(); reset() takes the account's values again.
//   fields    pronouns, bio, custom status, the profile colours, the
//             display-name style, and the display name where the app edits
//             it here (`editsName`; the desktop edits it under Account)
//   avatar    a picked file framed first (frameAvatar), a recent upload
//             (takeRecent), the current picture framed again, or removed
//   banner    a picked file (takeBanner) or removed
//   nameplate a picked picture or video (takeNameplate) or removed
//   checks    a picked image's type and size (check), the core's display
//             name check and the core's limits (`limits`)
//   save      the display name (when it changed) and the profile go to the
//             core; once every save that went out lands, `saved` is true
//             for a moment and the draft is the account again
QtObject {
    id: draft

    // The editor is up: a save that lands resets the draft only then.
    property bool active: false
    // The display name is part of this draft (the phone).
    property bool editsName: false

    // The core's limits in characters: {displayName, pronouns, bio,
    // customStatus}.
    readonly property var limits: JSON.parse(Settings.profileLimits())

    property string displayName
    property string pronouns
    property string bio
    property string customStatus
    // The status the draft took from the account: only a field changed from
    // it goes out (the account's can move while the editor is up).
    property string loadedStatus
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
    property bool cropEdited: false
    // The display-name style as the views carry it, or empty.
    property string nameStyle
    // A picture or video for the nameplate, or the upload to delete.
    property string nameplatePath
    property bool removeNameplate: false
    property string localError
    // A banner over its limit (the too-big card's JSON, under the banner).
    property string bannerTooBig
    property bool saved: false
    // Saves this draft sent that have not landed yet.
    property bool waitingName: false
    property bool waitingProfile: false

    readonly property bool nameChanged: editsName && displayName.trim() !== Settings.displayName
    readonly property bool statusChanged: customStatus.trim() !== loadedStatus.trim()
    readonly property bool profileDirty: pronouns !== Settings.pronouns
        || bio !== Settings.bio
        || statusChanged
        || accentColor !== Settings.accentColor
        || backgroundColor !== Settings.backgroundColor
        || bannerColor !== Settings.bannerColor
        || avatarPath.length > 0 || removeAvatar || avatarUrl.length > 0
        || bannerPath.length > 0 || removeBanner
        || cropEdited || nameStyle !== Session.meNameStyle
        || nameplatePath.length > 0 || removeNameplate
    readonly property bool dirty: profileDirty || nameChanged
    // The display name as the core would refuse it, or empty.
    readonly property string nameError: editsName ? Settings.checkDisplayName(displayName) : ""
    readonly property bool saving: Settings.profileSaving || (editsName && Settings.displayNameSaving)
    readonly property string error: localError.length > 0 ? localError
        : Settings.profileError.length > 0 || !editsName ? Settings.profileError
        : nameChanged ? Settings.displayNameError : ""

    // What the profile wears (collectibles apply at once, not on Save; a
    // removed nameplate waits for Save, removePlate).
    readonly property var worn: Session.meDisplay.length > 0 ? (JSON.parse(Session.meDisplay).collectibles ?? {}) : ({})
    readonly property string shownAvatar: removeAvatar ? "" : avatarPath.length > 0 ? fileUrl(avatarPath)
        : avatarUrlShown.length > 0 ? avatarUrlShown : Settings.avatar
    readonly property string shownBanner: removeBanner ? "" : bannerPath.length > 0 ? fileUrl(bannerPath) : Settings.banner
    // The stored crop, then the draft's (avatarDisplayToImageStyle).
    readonly property var storedCrop: {
        if (Session.meDisplay.length === 0)
            return null
        const display = JSON.parse(Session.meDisplay)
        return display && display.crop ? display.crop : null
    }
    readonly property var shownCrop: removeAvatar ? null : cropEdited ? crop : storedCrop
    readonly property string nameplateType: nameplatePath.length > 0 ? Settings.nameplateType(nameplatePath) : ""
    // The plate the preview wears: the picked file, else the saved one.
    readonly property string shownPlate: removeNameplate ? ""
        : nameplatePath.length > 0 ? JSON.stringify({
            still: nameplateType.startsWith("video/") ? "" : fileUrl(nameplatePath),
            motion: nameplateType.startsWith("video/") ? fileUrl(nameplatePath) : "",
            border: "", light: false, assist: true
        })
        : Session.mePlate
    // Remove has something to take off: a picked or recent picture, or the
    // saved custom one (or its crop).
    readonly property bool canRemoveAvatar: avatarPath.length > 0 || avatarUrl.length > 0
        || (!removeAvatar && (Settings.customAvatar || shownCrop !== null))
    readonly property bool canRemoveNameplate: nameplatePath.length > 0
        || (!removeNameplate && (Session.mePlate.length > 0 || worn.nameplate !== undefined))
    // The avatar and banner rows' lines.
    readonly property string avatarNote: removeAvatar ? qsTr("Removed when you save")
        : avatarPath.length > 0 ? qsTr("New image, uploads when you save")
        : avatarUrl.length > 0 ? qsTr("Recent image, applies when you save")
        : Settings.customAvatar ? qsTr("Your current avatar")
        : qsTr("Using your sign-in picture")
    readonly property string bannerNote: removeBanner ? qsTr("Removed when you save")
        : bannerPath.length > 0 ? qsTr("New image, uploads when you save")
        : Settings.banner.length > 0 ? qsTr("Your current banner")
        : qsTr("No banner. Your banner color shows instead.")

    // A picked file's URL: `C:/…` on Windows, `/…` on Android.
    function fileUrl(path) {
        return path.startsWith("/") ? "file://" + path : "file:///" + path
    }

    // Everything but the display name, from the account.
    function resetProfile() {
        pronouns = Settings.pronouns
        bio = Settings.bio
        customStatus = Settings.customStatus
        loadedStatus = customStatus
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
        cropEdited = false
        nameStyle = Session.meNameStyle
        nameplatePath = ""
        removeNameplate = false
        localError = ""
        bannerTooBig = ""
        saved = false
    }
    function reset() {
        resetProfile()
        displayName = Settings.displayName
    }
    // The display name goes out when it changed; the profile when anything
    // else did, or when nothing did (a save again after an error).
    function save() {
        localError = ""
        if (nameError.length > 0) {
            localError = nameError
            return
        }
        const name = nameChanged
        if (name) {
            waitingName = true
            Settings.saveDisplayName(displayName)
        }
        if (name && !profileDirty)
            return
        waitingProfile = true
        Settings.saveProfile(JSON.stringify({
            pronouns: pronouns,
            bio: bio,
            customStatus: customStatus,
            customStatusChanged: statusChanged,
            accentColor: accentColor,
            backgroundColor: backgroundColor,
            bannerColor: bannerColor,
            avatarPath: avatarPath,
            removeAvatar: removeAvatar,
            avatarUrl: avatarUrl,
            bannerPath: bannerPath,
            removeBanner: removeBanner,
            crop: crop,
            cropChanged: cropEdited,
            nameStyle: nameStyle,
            nameStyleChanged: nameStyle !== Session.meNameStyle,
            nameplatePath: nameplatePath,
            removeNameplate: removeNameplate,
        }))
    }

    // A picked file for the avatar or the banner. Not an image: null, with
    // the error copy in `localError`. Else the too-big card's JSON, empty
    // when it fits (a banner's also goes in `bannerTooBig`). Nothing over
    // its limit joins the draft, so nothing over it is uploaded; the plan
    // sets the limit (25 MB with bigUploads, else 8 MB).
    function check(kind, path) {
        if (Settings.imageType(path).length === 0) {
            localError = kind === "avatar" ? qsTr("Choose an image file for your avatar.") : qsTr("Choose an image file for your banner.")
            return null
        }
        const tooBig = Limits.checkUpTo(kind, path, kind === "avatar" ? Settings.avatarLimit : Settings.bannerLimit)
        if (kind === "banner")
            bannerTooBig = tooBig
        return tooBig
    }
    // The frame editor's Apply: a picked file joins the draft framed, or
    // (no file) the current picture is framed again.
    function frameAvatar(filePath, frame) {
        if (filePath.length > 0) {
            avatarPath = filePath
            avatarUrl = ""
            avatarUrlShown = ""
        }
        removeAvatar = false
        crop = frame
        cropEdited = true
    }
    // One of the recent uploads again (`stored` is what the save sends,
    // `url` its picture); it starts unframed (withoutCrop).
    function takeRecent(stored, url) {
        localError = ""
        avatarPath = ""
        removeAvatar = false
        avatarUrl = stored
        avatarUrlShown = url
        crop = null
        cropEdited = true
    }
    // Remove: a picked or recent picture is dropped, else the saved one
    // comes off on Save.
    function dropAvatar() {
        localError = ""
        crop = null
        cropEdited = false
        if (avatarPath.length > 0 || avatarUrl.length > 0) {
            avatarPath = ""
            avatarUrl = ""
            avatarUrlShown = ""
        } else {
            removeAvatar = true
        }
    }
    function takeBanner(path) {
        localError = ""
        bannerPath = path
        removeBanner = false
    }
    function dropBanner() {
        if (bannerPath.length > 0)
            bannerPath = ""
        else
            removeBanner = true
    }
    // A nameplate (handleNameplateSelect): a picture or a video.
    function takeNameplate(path) {
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

    // Every save that went out landed, and the name (if it went) was taken:
    // the draft is the profile now.
    function finish() {
        if (waitingName || waitingProfile || (nameChanged && Settings.displayNameError.length > 0))
            return
        saved = true
        savedTimer.restart()
    }

    property Connections saves: Connections {
        target: Settings
        function onProfileSavedChanged() {
            if (!draft.active)
                return
            draft.waitingProfile = false
            draft.resetProfile()
            draft.finish()
        }
        function onDisplayNameSavingChanged() {
            if (Settings.displayNameSaving || !draft.waitingName)
                return
            draft.waitingName = false
            // Refused: the draft keeps the name, and the error shows.
            if (Settings.displayNameError.length > 0)
                return
            draft.displayName = Settings.displayName
            if (draft.active)
                draft.finish()
        }
    }
    property Timer savedTimer: Timer {
        interval: 2400
        onTriggered: draft.saved = false
    }
}
