use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

/// Every QML file of `com.yap.native`; the type name is the file name, so
/// names must be unique across folders.
const QML: &[&str] = &[
    "qml/Main.qml",
    "qml/chrome/TitleBar.qml",
    "qml/chrome/WindowKey.qml",
    "qml/components/Avatar.qml",
    "qml/components/CallKey.qml",
    "qml/components/Icon.qml",
    "qml/components/KeyButton.qml",
    "qml/components/Led.qml",
    "qml/components/RailPill.qml",
    "qml/delegates/CategoryLabel.qml",
    "qml/delegates/ChannelRow.qml",
    "qml/delegates/MemberRow.qml",
    "qml/delegates/MessageDelegate.qml",
    "qml/delegates/ServerIcon.qml",
    "qml/delegates/TreeRow.qml",
    "qml/screens/Boot/BootView.qml",
    "qml/screens/SignIn/SignInView.qml",
    "qml/screens/Shell/CallPanel.qml",
    "qml/screens/Shell/ChannelSidebar.qml",
    "qml/screens/Shell/ChatView.qml",
    "qml/screens/Shell/Composer.qml",
    "qml/screens/Shell/ServerRail.qml",
    "qml/screens/Shell/Shell.qml",
];

fn main() {
    let files = QML
        .iter()
        .map(|path| QmlFile::from(*path))
        .chain(std::iter::once(
            QmlFile::from("qml/theme/Theme.qml").singleton(true),
        ));
    CxxQtBuilder::new_qml_module(QmlModule::new("com.yap.native").qml_files(files))
        .files(["src/bridge.rs", "src/models.rs"])
        .build();
}
