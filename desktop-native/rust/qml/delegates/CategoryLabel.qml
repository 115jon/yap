import QtQuick
import com.yap.native

// A tree branch label (sidebar-row.ts TREE_BRANCH_LABEL): a fixed 24 px
// silkscreen row in ink-3, the 12 px chevron in the 20 px icon column so the
// label starts on the channel-name edge; 16 px above it, 8 for the first.
Item {
    id: root

    required property string name
    required property bool first

    // The list puts 2 px between rows; the gap above makes up the rest.
    height: Theme.branchLabel + (first ? 8 : 14)
    Accessible.role: Accessible.Heading
    Accessible.name: name

    Icon {
        x: 8 + 4
        anchors.verticalCenter: label.verticalCenter
        name: "chevronDown"
        size: Theme.iconXs
        color: Theme.ink3
    }
    Text {
        id: label
        anchors.left: parent.left
        anchors.leftMargin: 8 + 20 + 8
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.bottom: parent.bottom
        height: Theme.branchLabel
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
        text: root.name
        color: Theme.ink3
        font: Theme.type.silk
    }
}
