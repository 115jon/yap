use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

fn main() {
    // The desktop's size settings for its QML (rust/build.rs explains them),
    // read by the patched qt-build-utils in this same process.
    if std::env::var_os("CARGO_FEATURE_SLIM_QML").is_some() {
        std::env::set_var("QT_BUILD_UTILS_QMLCACHEGEN_BYTECODE_ONLY", "*");
        std::env::set_var("QT_BUILD_UTILS_DISCARD_QML_CONTENTS", "1");
    }
    let files = [QmlFile::from("qml/Theme.qml").singleton(true)];
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
