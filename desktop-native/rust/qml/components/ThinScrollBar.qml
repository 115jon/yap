import QtQuick
import QtQuick.Templates as T
import com.yap.native

// The app's scrollbar (styles/foundation.css): an 8 px gutter with no
// track and a 4 px pill, tint-2 (tint-3 hovered or dragged). Like the web's
// classic bar it shows only while the content overflows, and then stays.
T.ScrollBar {
    id: root

    implicitWidth: orientation === Qt.Vertical ? 8 : implicitContentWidth + leftPadding + rightPadding
    implicitHeight: orientation === Qt.Horizontal ? 8 : implicitContentHeight + topPadding + bottomPadding
    padding: 2
    hoverEnabled: true
    minimumSize: 0.05
    policy: T.ScrollBar.AsNeeded
    visible: size < 1

    contentItem: Rectangle {
        implicitWidth: 4
        implicitHeight: 4
        radius: 2
        color: root.hovered || root.pressed ? Theme.tint3 : Theme.tint2
    }
}
