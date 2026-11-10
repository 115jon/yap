pragma Singleton
import QtQuick

// The desktop's `Panels` (pins, threads): not on mobile yet.
QtObject {
    readonly property bool canPin: false
}
