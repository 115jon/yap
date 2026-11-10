import QtQuick
import com.yap.native

// A widget with nothing to show (shared.tsx WidgetEmpty): a quiet glyph,
// one line and a hint, centred down the plate's body. No art, no action.
// Put it straight in a WidgetPlate.
Column {
    id: root

    property string icon: "sparkle"
    property string title: ""
    property string hint: ""

    width: parent ? parent.width : 0
    y: parent ? Math.max(0, Math.round((parent.height - implicitHeight) / 2)) : 0
    spacing: 4

    Icon {
        name: root.icon
        size: Theme.iconLg
        color: Theme.ink3
    }
    Item { width: 1; height: 2 }
    Text {
        width: parent.width
        textFormat: Text.PlainText
        text: root.title
        wrapMode: Text.Wrap
        maximumLineCount: 2
        elide: Text.ElideRight
        color: Theme.ink2
        font: Theme.type.occupant
    }
    Text {
        visible: root.hint.length > 0
        width: parent.width
        textFormat: Text.PlainText
        text: root.hint
        wrapMode: Text.Wrap
        maximumLineCount: 3
        elide: Text.ElideRight
        color: Theme.ink3
        font: Theme.type.caption
    }
}
