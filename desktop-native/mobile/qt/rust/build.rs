use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

/// The mobile shell (`com.yap.mobile`): the window, the pager and its pages,
/// and the mobile side of each seam the shared QML (`Yap.Ui`, from the
/// yap-ui crate) imports from `Yap.Platform`: rich embeds, media, the voice
/// status line, the stream card and the nameplate. main.cpp registers those five under
/// Yap.Platform.
const QML: &[&str] = &[
    "qml/Main.qml",
    "qml/SignInPage.qml",
    "qml/Pager.qml",
    "qml/DrawerPage.qml",
    "qml/AccountBar.qml",
    "qml/DmsPane.qml",
    "qml/PlatformList.qml",
    "qml/ChatPage.qml",
    "qml/MembersPage.qml",
    "qml/ProfilePage.qml",
    "qml/SettingsPage.qml",
    "qml/FriendsPage.qml",
    "qml/ThreadPage.qml",
    "qml/PageHeader.qml",
    "qml/MobileField.qml",
    "qml/InlineConfirm.qml",
    "qml/MobileComposer.qml",
    "qml/UploadTray.qml",
    "qml/ImageViewerPage.qml",
    "qml/CallPage.qml",
    "qml/CallBar.qml",
    "qml/DmCallStrip.qml",
    "qml/IncomingCallSheet.qml",
    "qml/BottomSheet.qml",
    "qml/SheetRow.qml",
    "qml/ServerSheet.qml",
    "qml/InviteLandingPage.qml",
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
    "qml/SwipeReply.qml",
    "qml/seams/EmbedCard.qml",
    "qml/seams/MessageMedia.qml",
    "qml/seams/NameplateLayer.qml",
    "qml/seams/VoiceStatusLine.qml",
    "qml/seams/VoiceStreamHoverCard.qml",
];

fn main() {
    let files = QML
        .iter()
        .map(|path| QmlFile::from(*path))
        // This phone's own settings (QSettings).
        .chain(std::iter::once(QmlFile::from("qml/Prefs.qml").singleton(true)));
    CxxQtBuilder::new_qml_module(QmlModule::new("com.yap.mobile").qml_files(files)).build();
}
