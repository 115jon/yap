import QtQuick
import com.yap.native

// An inline text action ("Retry", "Esc to cancel"): 12 px in accent ink,
// underlined on hover.
Text {
    id: link

    signal activated()

    textFormat: Text.PlainText

    color: Theme.accentInk
    font.family: Theme.type.captionStrong.family
    font.pixelSize: Theme.type.captionStrong.pixelSize
    font.weight: Theme.type.captionStrong.weight
    font.underline: hover.hovered
    activeFocusOnTab: true

    Accessible.role: Accessible.Link
    Accessible.name: text
    Accessible.onPressAction: link.activated()
    Keys.onReturnPressed: link.activated()
    Keys.onSpacePressed: link.activated()

    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: link.activated() }
}
