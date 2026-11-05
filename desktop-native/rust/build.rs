use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

/// Every QML file of `com.yap.native`; the type name is the file name, so
/// names must be unique across folders.
const QML: &[&str] = &[
    "qml/Main.qml",
    "qml/chrome/TitleBar.qml",
    "qml/chrome/WindowKey.qml",
    "qml/components/Avatar.qml",
    "qml/components/CallKey.qml",
    "qml/components/CountBadge.qml",
    "qml/components/Icon.qml",
    "qml/components/IconKey.qml",
    "qml/components/KeyButton.qml",
    "qml/components/Led.qml",
    "qml/components/LinkText.qml",
    "qml/components/MenuPanel.qml",
    "qml/components/MenuRow.qml",
    "qml/components/MessageEditor.qml",
    "qml/components/NotchedKey.qml",
    "qml/components/PresenceAvatar.qml",
    "qml/components/RailPill.qml",
    "qml/components/Tooltip.qml",
    "qml/components/YapBark.qml",
    "qml/delegates/CategoryLabel.qml",
    "qml/delegates/ChannelRow.qml",
    "qml/delegates/DmRow.qml",
    "qml/delegates/LandingMember.qml",
    "qml/delegates/MemberEntry.qml",
    "qml/delegates/MemberRow.qml",
    "qml/delegates/MessageDelegate.qml",
    "qml/delegates/ServerIcon.qml",
    "qml/delegates/TreeRow.qml",
    "qml/delegates/VoiceTile.qml",
    "qml/screens/Boot/BootView.qml",
    "qml/screens/SignIn/SignInView.qml",
    "qml/screens/Shell/AccountPopout.qml",
    "qml/screens/Shell/AccountRow.qml",
    "qml/screens/Shell/CallPanel.qml",
    "qml/screens/Shell/ChannelSidebar.qml",
    "qml/screens/Shell/ChatHeader.qml",
    "qml/screens/Shell/ChatView.qml",
    "qml/screens/Shell/Composer.qml",
    "qml/screens/Shell/ConnectionStrip.qml",
    "qml/screens/Shell/DeleteDialog.qml",
    "qml/screens/Shell/DmSidebar.qml",
    "qml/screens/Shell/HomeLanding.qml",
    "qml/screens/Shell/MemberPanel.qml",
    "qml/screens/Shell/ProfileCard.qml",
    "qml/screens/Shell/ProfilePopout.qml",
    "qml/screens/Shell/ServerRail.qml",
    "qml/screens/Shell/Shell.qml",
    "qml/screens/Shell/UnreadBanner.qml",
    "qml/screens/Voice/VoiceView.qml",
];

fn main() {
    let files = QML.iter().map(|path| QmlFile::from(*path)).chain([
        QmlFile::from("qml/theme/Theme.qml").singleton(true),
        QmlFile::from("qml/screens/Shell/ShellState.qml").singleton(true),
    ]);
    CxxQtBuilder::new_qml_module(QmlModule::new("com.yap.native").qml_files(files))
        .files(["src/bridge.rs", "src/models.rs"])
        .build();
}
