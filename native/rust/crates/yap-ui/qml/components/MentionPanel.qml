import QtQuick
import Yap.Ui

// Member suggestions for an `@` in a composer (MentionAutocomplete.tsx), on
// MentionListModel: up to five members, their face, display name in their
// style and `@username`. Both composers place it over the messages and
// feed it: `tokenBefore` finds the `@` token at the caret, Chat.
// queryMentions fills the model, `picked` puts `@username ` in. The
// desktop's keys move `currentIndex` (and the pointer moves it by
// `pointed`); a finger taps a row. It fades in and out at dur-1, and no tap
// on it reaches what lies under it.
Rectangle {
    id: root

    property bool open: false
    // The keyboard's pick, highlighted (-1: none).
    property int currentIndex: -1
    // The silkscreen "Members" heading over the rows.
    property bool heading: true

    signal picked(string username)
    // The pointer came to rest on a row.
    signal pointed(int index)

    readonly property int count: suggestions.count

    // The desktop's rows (36 px, a 20 px face), or a thumb's (Theme.touch:
    // 48dp, a 28dp face).
    property int rowHeight: Theme.touch ? 48 : 36
    property int faceSize: Theme.touch ? 28 : 20
    property int nameX: Theme.touch ? 48 : 36
    property int edge: Theme.touch ? 12 : 8

    // The `@` token that ends `before` (the text up to the caret): where
    // its `@` is and what follows it, or null when the caret is not in one.
    function tokenBefore(before) {
        const match = /(^|\s)@([^\s@]*)$/.exec(before)
        return match ? { start: before.length - match[2].length - 1, query: match[2] } : null
    }
    // The username on row `index`, or empty.
    function usernameAt(index) {
        const row = suggestions.itemAtIndex(index)
        return row ? row.username : ""
    }

    opacity: open ? 1 : 0
    EaseNumber on opacity {}
    visible: opacity > 0
    height: suggestions.contentHeight + (heading ? 32 : 8)
    radius: Theme.radius
    color: Theme.ply3
    border.width: 1
    border.color: Theme.lineStrong
    Accessible.role: Accessible.List
    Accessible.name: qsTr("Members")

    TapHandler {
        gesturePolicy: TapHandler.ReleaseWithinBounds
    }

    Text {
        visible: root.heading
        x: 12
        y: 8
        text: qsTr("Members")
        color: Theme.ink3
        font: Theme.type.silk
    }
    ListView {
        id: suggestions
        x: 4
        y: root.heading ? 28 : 4
        width: parent.width - 8
        height: contentHeight
        interactive: false
        model: MentionListModel
        currentIndex: root.currentIndex
        delegate: Rectangle {
            id: suggestion
            required property int index
            required property string username
            required property string name
            required property string avatar
            required property string initials
            required property string nameStyle
            width: suggestions.width
            height: root.rowHeight
            radius: Theme.radius
            color: index === root.currentIndex || (Theme.touch && tap.pressed) ? Theme.tint2 : "transparent"
            Accessible.role: Theme.touch ? Accessible.Button : Accessible.ListItem
            Accessible.name: Theme.touch ? qsTr("Mention %1").arg(suggestion.name) : suggestion.name
            Accessible.onPressAction: root.picked(suggestion.username)
            Avatar {
                x: 8
                anchors.verticalCenter: parent.verticalCenter
                size: root.faceSize
                initials: suggestion.initials
                source: suggestion.avatar
            }
            // MentionAutocomplete.tsx: UserDisplayName, in their style.
            DisplayName {
                x: root.nameX
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, parent.width - x - handle.width - 20)
                text: suggestion.name
                playing: hover.hovered
                nameStyle: suggestion.nameStyle
                color: Theme.ink1
                font: Theme.type.row
                ground: Theme.ply3
            }
            Text {
                id: handle
                anchors.right: parent.right
                anchors.rightMargin: root.edge
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: "@" + suggestion.username
                color: Theme.ink3
                font: Theme.type.caption
            }
            HoverHandler {
                id: hover
                enabled: Theme.hoverUi
                cursorShape: Qt.PointingHandCursor
                onHoveredChanged: if (hovered) root.pointed(suggestion.index)
            }
            TapHandler {
                id: tap
                gesturePolicy: TapHandler.ReleaseWithinBounds
                onTapped: root.picked(suggestion.username)
            }
        }
    }
}
