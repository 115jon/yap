use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

const QML: &[&str] = &[
    "qml/Main.qml",
    "qml/SignInView.qml",
    "qml/ServerRail.qml",
    "qml/ChannelSidebar.qml",
    "qml/ChatView.qml",
    "qml/MessageDelegate.qml",
    "qml/Composer.qml",
    "qml/Avatar.qml",
];

fn main() {
    let files = QML
        .iter()
        .map(|path| QmlFile::from(*path))
        .chain(std::iter::once(
            QmlFile::from("qml/Theme.qml").singleton(true),
        ));
    CxxQtBuilder::new_qml_module(QmlModule::new("com.yap.native").qml_files(files))
        .files(["src/bridge.rs"])
        .build();
}
