pragma Singleton
import QtQuick

// The desktop's `Admin` singleton, as far as the shared rows read it:
// server management is not in the mobile prototype.
QtObject {
    readonly property bool canManageChannels: false
}
