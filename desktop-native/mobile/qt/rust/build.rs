use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

/// The mobile shell (`com.yap.mobile`): the window, the pager and its pages,
/// and the mobile side of each seam the shared QML (`Yap.Ui`, from the
/// yap-ui crate) imports from `Yap.Platform`: rich embeds, media, the voice
/// status line and the stream card. main.cpp registers those four under
/// Yap.Platform.
const QML: &[&str] = &[
    "qml/Main.qml",
    "qml/SignInPage.qml",
    "qml/Pager.qml",
    "qml/DrawerPage.qml",
    "qml/AccountBar.qml",
    "qml/DmsPane.qml",
    "qml/DmItem.qml",
    "qml/PlatformList.qml",
    "qml/ChatPage.qml",
    "qml/MembersPage.qml",
    "qml/MemberItem.qml",
    "qml/ProfilePage.qml",
    "qml/SettingsPage.qml",
    "qml/MobileComposer.qml",
    "qml/UploadTray.qml",
    "qml/ImageViewerPage.qml",
    "qml/seams/EmbedCard.qml",
    "qml/seams/MessageMedia.qml",
    "qml/seams/VoiceStatusLine.qml",
    "qml/seams/VoiceStreamHoverCard.qml",
];

fn main() {
    let files = QML
        .iter()
        .map(|path| QmlFile::from(*path))
        // Each channel's unsent text, kept across chat pages.
        .chain(std::iter::once(QmlFile::from("qml/Drafts.qml").singleton(true)));
    CxxQtBuilder::new_qml_module(QmlModule::new("com.yap.mobile").qml_files(files)).build();
}
