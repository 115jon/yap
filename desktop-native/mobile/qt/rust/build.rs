use std::fs;
use std::path::Path;

use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

/// The desktop's QML that runs here unchanged (paths under
/// `desktop-native/rust/`). They join `com.yap.native` next to the mobile
/// files below, as on the desktop, so their `import com.yap.native` and
/// their use of `Theme`, `Chat` or `Nav` resolve the same way.
const SHARED_QML: &[&str] = &[
    "qml/components/Avatar.qml",
    "qml/components/CountBadge.qml",
    "qml/components/DisplayName.qml",
    "qml/components/EaseColor.qml",
    "qml/components/EaseNumber.qml",
    "qml/components/FloatShadow.qml",
    "qml/components/Icon.qml",
    "qml/components/IconKey.qml",
    "qml/components/KitButton.qml",
    "qml/components/LazyPopup.qml",
    "qml/components/Led.qml",
    "qml/components/LinkText.qml",
    "qml/components/ListEnter.qml",
    "qml/components/MessageEditor.qml",
    "qml/components/PresenceIn.qml",
    "qml/components/PresenceOut.qml",
    "qml/components/RailPill.qml",
    "qml/components/Spinner.qml",
    "qml/components/Tooltip.qml",
    "qml/delegates/CategoryLabel.qml",
    "qml/delegates/ChannelRow.qml",
    "qml/delegates/ListenSidebarStatus.qml",
    "qml/delegates/MemberRow.qml",
    "qml/delegates/MessageDelegate.qml",
    "qml/delegates/ReactionStrip.qml",
    "qml/delegates/ServerIcon.qml",
    "qml/delegates/TreeRow.qml",
    "qml/screens/Admin/ClipboardText.qml",
];

/// Shared singletons (same source, `pragma Singleton`).
const SHARED_SINGLETONS: &[&str] = &["qml/screens/Shell/ShellState.qml"];

/// The desktop's Rust that compiles here unchanged: the list models. It
/// lands in `src/` (gitignored there): cxx-qt-build takes the bridge files
/// of a QML module from one folder (QTBUG-93443).
const SHARED_RUST: &[&str] = &["src/models.rs"];

/// The mobile shell, and the mobile side of each seam the shared QML
/// reaches: rich embeds, media and voice (not in the prototype yet) and the
/// desktop-only singletons, each a small file with the desktop's API.
const MOBILE_QML: &[&str] = &[
    "qml/Main.qml",
    "qml/SignInPage.qml",
    "qml/ServersPage.qml",
    "qml/PlatformList.qml",
    "qml/ChatPage.qml",
    "qml/MobileComposer.qml",
    "qml/seams/EmbedCard.qml",
    "qml/seams/MessageMedia.qml",
    "qml/seams/VoiceStatusLine.qml",
    "qml/seams/VoiceStreamHoverCard.qml",
];

const MOBILE_SINGLETONS: &[&str] = &[
    "qml/seams/Admin.qml",
    "qml/seams/Apps.qml",
    "qml/seams/Emoji.qml",
    "qml/seams/Extras.qml",
    "qml/seams/Invites.qml",
    "qml/seams/Panels.qml",
    "qml/seams/Profile.qml",
    "qml/seams/Voice.qml",
];

/// Copies a desktop file into `shared/` (gitignored) when it changed:
/// qt-build-utils names each resource by its path, which must stay inside
/// the crate. The desktop file stays the only source.
fn mirror(desktop: &Path, rel: &str) -> String {
    mirror_to(desktop, rel, &Path::new("shared").join(rel))
}

fn mirror_to(desktop: &Path, rel: &str, to: &Path) -> String {
    let from = desktop.join(rel);
    println!("cargo:rerun-if-changed={}", from.display());
    let text = fs::read(&from).unwrap_or_else(|e| panic!("{}: {e}", from.display()));
    if fs::read(to).ok().as_deref() != Some(text.as_slice()) {
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::write(to, text).unwrap();
    }
    to.to_str().unwrap().replace('\\', "/")
}

fn main() {
    let desktop = Path::new("../../../rust");
    for rel in SHARED_RUST {
        mirror_to(desktop, &format!("crates/yap-ui/{rel}"), Path::new(rel));
    }
    let files = SHARED_QML
        .iter()
        .map(|rel| QmlFile::from(mirror(desktop, rel)))
        .chain(
            SHARED_SINGLETONS
                .iter()
                .map(|rel| QmlFile::from(mirror(desktop, rel)).singleton(true)),
        )
        .chain(MOBILE_QML.iter().map(|path| QmlFile::from(*path)))
        .chain(
            MOBILE_SINGLETONS
                .iter()
                .map(|path| QmlFile::from(*path).singleton(true)),
        );
    CxxQtBuilder::new_qml_module(QmlModule::new("com.yap.native").qml_files(files))
        .files(["src/bridge.rs", "src/models.rs"])
        .build();
}
