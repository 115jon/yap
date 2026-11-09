import QtQuick
import com.yap.native

// A group label inside the search panel (SearchPanel.tsx GROUP_LABEL):
// sentence case, 12/500, ink-3, px-2 pt-2 pb-1; 12 more above each group
// after the first.
Item {
    id: root

    property string text
    property bool first: false

    implicitHeight: (first ? 0 : 12) + 8 + label.implicitHeight + 4
    Accessible.role: Accessible.Heading
    Accessible.name: text

    Text {
        id: label
        x: 8
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 4
        width: parent.width - 16
        textFormat: Text.PlainText
        text: root.text
        elide: Text.ElideRight
        color: Theme.ink3
        font: Theme.type.captionStrong
    }
}
