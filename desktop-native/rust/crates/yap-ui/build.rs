use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

fn main() {
    // The desktop's size settings for its QML (rust/build.rs explains them),
    // read by the patched qt-build-utils in this same process.
    if std::env::var_os("CARGO_FEATURE_SLIM_QML").is_some() {
        std::env::set_var("QT_BUILD_UTILS_QMLCACHEGEN_BYTECODE_ONLY", "*");
        std::env::set_var("QT_BUILD_UTILS_DISCARD_QML_CONTENTS", "1");
    }
    // The QML both apps show: tokens, shell state, and the rows and
    // components they are made of. What differs per app (rich embeds,
    // media, voice status, the stream card, a row's nameplate) is imported
    // from Yap.Platform, which each app registers (its main.cpp).
    let files = [
        QmlFile::from("qml/Theme.qml").singleton(true),
        QmlFile::from("qml/ShellState.qml").singleton(true),
        QmlFile::from("qml/components/Avatar.qml"),
        QmlFile::from("qml/components/CountBadge.qml"),
        QmlFile::from("qml/components/DisplayName.qml"),
        QmlFile::from("qml/components/EaseColor.qml"),
        QmlFile::from("qml/components/EaseNumber.qml"),
        QmlFile::from("qml/components/ConnectionStrip.qml"),
        QmlFile::from("qml/components/FloatShadow.qml"),
        QmlFile::from("qml/components/GameLine.qml"),
        QmlFile::from("qml/components/Icon.qml"),
        QmlFile::from("qml/components/IconKey.qml"),
        QmlFile::from("qml/components/KitButton.qml"),
        QmlFile::from("qml/components/LazyPopup.qml"),
        QmlFile::from("qml/components/Led.qml"),
        QmlFile::from("qml/components/LinkText.qml"),
        QmlFile::from("qml/components/ListEnter.qml"),
        QmlFile::from("qml/components/MentionPanel.qml"),
        QmlFile::from("qml/components/MessageEditor.qml"),
        QmlFile::from("qml/components/PlatformIcons.qml"),
        QmlFile::from("qml/components/PresenceAvatar.qml"),
        QmlFile::from("qml/components/PresenceIn.qml"),
        QmlFile::from("qml/components/PresenceOut.qml"),
        QmlFile::from("qml/components/RailPill.qml"),
        QmlFile::from("qml/components/SettingSwitch.qml"),
        QmlFile::from("qml/components/Spinner.qml"),
        QmlFile::from("qml/components/TabRule.qml"),
        QmlFile::from("qml/components/Tooltip.qml"),
        QmlFile::from("qml/components/UnreadBanner.qml"),
        QmlFile::from("qml/delegates/CategoryLabel.qml"),
        QmlFile::from("qml/delegates/ChannelRow.qml"),
        QmlFile::from("qml/delegates/DmRow.qml"),
        QmlFile::from("qml/delegates/ListenSidebarStatus.qml"),
        QmlFile::from("qml/delegates/MemberEntry.qml"),
        QmlFile::from("qml/delegates/MemberRow.qml"),
        QmlFile::from("qml/delegates/MessageDelegate.qml"),
        QmlFile::from("qml/delegates/ReactionStrip.qml"),
        QmlFile::from("qml/delegates/ServerIcon.qml"),
        QmlFile::from("qml/delegates/TreeRow.qml"),
        QmlFile::from("qml/components/ClipboardText.qml"),
    ];
    // The bridge files of one QML module live in one folder (src/, flat):
    // cxx-qt-build's rule (QTBUG-93443).
    CxxQtBuilder::new_qml_module(QmlModule::new("Yap.Ui").qml_files(files))
        .files([
            "src/admin.rs",
            "src/apps.rs",
            "src/board.rs",
            "src/bridge.rs",
            "src/calls.rs",
            "src/color.rs",
            "src/command_menu.rs",
            "src/extras.rs",
            "src/friends.rs",
            "src/invites.rs",
            "src/limits.rs",
            "src/models.rs",
            "src/panels.rs",
            "src/picker.rs",
            "src/search.rs",
            "src/settings.rs",
            "src/shop.rs",
        ])
        .build()
        .export();
}
