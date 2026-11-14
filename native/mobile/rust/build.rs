use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

/// The mobile shell (`com.yap.mobile`): the window, the pager and its pages,
/// and the mobile side of each seam the shared QML (`Yap.Ui`, from the
/// yap-ui crate) imports from `Yap.Platform`: the embed cards' web player,
/// video and sound, media, the voice status line, the stream card, the
/// nameplate and the call's video surface. main.cpp registers those eight
/// under Yap.Platform.
const QML: &[&str] = &[
    "qml/Main.qml",
    "qml/SignInPage.qml",
    "qml/Pager.qml",
    "qml/DrawerPage.qml",
    "qml/AccountPanel.qml",
    "qml/AccountPlate.qml",
    "qml/DmsPane.qml",
    "qml/PlatformList.qml",
    "qml/ChatPage.qml",
    "qml/MembersPage.qml",
    "qml/ProfilePage.qml",
    "qml/SettingsPage.qml",
    "qml/ProfileEditPage.qml",
    "qml/AvatarFramePage.qml",
    "qml/FriendsPage.qml",
    "qml/ThreadPage.qml",
    "qml/SearchPage.qml",
    "qml/PageHeader.qml",
    "qml/MobileField.qml",
    "qml/InlineConfirm.qml",
    "qml/MobileComposer.qml",
    "qml/UploadTray.qml",
    "qml/ImageViewerPage.qml",
    "qml/CallPage.qml",
    "qml/CallVideo.qml",
    "qml/DmCallStrip.qml",
    "qml/IncomingCallSheet.qml",
    "qml/AttachSheet.qml",
    "qml/BottomSheet.qml",
    "qml/SheetRow.qml",
    "qml/ServerSheet.qml",
    "qml/InviteLandingPage.qml",
    "qml/SharePickerPage.qml",
    "qml/EmojiGlyph.qml",
    "qml/MessageActions.qml",
    "qml/MessageSheet.qml",
    "qml/EmojiPane.qml",
    "qml/EmojiSheet.qml",
    "qml/GifPane.qml",
    "qml/PickerSheet.qml",
    "qml/ReactionSheet.qml",
    "qml/ConfirmSheet.qml",
    "qml/SelectTextPage.qml",
    "qml/LicensesPage.qml",
    "qml/SwipeReply.qml",
    "qml/seams/InlineAudio.qml",
    "qml/seams/InlineVideo.qml",
    "qml/seams/MessageMedia.qml",
    "qml/seams/NameplateLayer.qml",
    "qml/seams/VideoSurface.qml",
    "qml/seams/VoiceStatusLine.qml",
    "qml/seams/VoiceStreamHoverCard.qml",
    "qml/seams/WebEmbedPlayer.qml",
];

fn main() {
    let files = QML
        .iter()
        .map(|path| QmlFile::from(*path))
        // This phone's own settings (QSettings), the page router and the
        // camera's link to the call.
        .chain([
            QmlFile::from("qml/Prefs.qml").singleton(true),
            QmlFile::from("qml/Router.qml").singleton(true),
            QmlFile::from("qml/CameraLink.qml").singleton(true),
        ]);
    CxxQtBuilder::new_qml_module(QmlModule::new("com.yap.mobile").qml_files(files)).build();
}
