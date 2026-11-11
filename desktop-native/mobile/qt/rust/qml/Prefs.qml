pragma Singleton
import QtQuick
import QtCore as QC

// What this phone keeps for itself, in the app's QSettings (main.cpp names
// the organisation and the app): settings with no field on the account or
// in the core's per-device files.
QC.Settings {
    category: "phone"

    // Settings > Text size: a factor over the system's font scale
    // (Native.fontScale); Main.qml multiplies the two into Theme.textScale.
    property real textFactor: 1
}
