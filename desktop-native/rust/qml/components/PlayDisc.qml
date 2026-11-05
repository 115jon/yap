import QtQuick
import com.yap.native

// The play disc (MediaPlayButton.tsx): 48 px, ply-3 with a seam, the
// ink-1 play glyph nudged 1 px right; ply-4 on hover.
Rectangle {
    property bool hovered: false

    width: 48
    height: 48
    radius: 24
    color: hovered ? Theme.ply4 : Theme.ply3
    border.width: 1
    border.color: Theme.line
    Accessible.ignored: true

    Icon {
        anchors.centerIn: parent
        anchors.horizontalCenterOffset: 1
        name: "play"
        variant: "active"
        size: Theme.iconXl
        color: Theme.ink1
    }
}
