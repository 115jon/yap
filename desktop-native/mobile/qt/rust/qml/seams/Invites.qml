pragma Singleton
import QtQuick

// The desktop's `Invites`: an invite link opens in the browser for now.
QtObject {
    function openLink(url) {
        Qt.openUrlExternally(url)
        return true
    }
}
