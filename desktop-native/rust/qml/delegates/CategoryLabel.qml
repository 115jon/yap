import QtQuick
import com.yap.native

// A tree branch label (sidebar-row.ts TREE_BRANCH_LABEL): a fixed 24 px
// silkscreen row in ink-3, the 12 px chevron in the 20 px icon column so the
// label starts on the channel-name edge; 16 px above it, 8 for the first.
// A click collapses the category or expands it (ChannelSidebar.tsx
// ChannelCategoryGroup): the chevron turns -90 degrees over dur-2, and the
// name goes to ink-2 under the pointer.
Item {
    id: root

    required property string name
    required property bool first
    property bool collapsed: false
    signal menuRequested(Item row, point at)
    signal toggled()

    // The list puts 2 px between rows; the gap above makes up the rest.
    height: Theme.branchLabel + (first ? 8 : 14)
    Accessible.role: Accessible.Button
    Accessible.name: collapsed ? qsTr("Expand %1").arg(name) : qsTr("Collapse %1").arg(name)
    Accessible.onPressAction: root.toggled()

    // The row's hit area: the label's 24 px line, as the web's button.
    Item {
        id: hit
        anchors.bottom: parent.bottom
        width: parent.width
        height: Theme.branchLabel
        HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: root.toggled() }
    }
    Icon {
        x: 8 + 4
        anchors.verticalCenter: label.verticalCenter
        name: "chevronDown"
        size: Theme.iconXs
        color: Theme.ink3
        rotation: root.collapsed ? -90 : 0
        // Turned, the raster is resampled.
        smooth: rotation !== 0
        Behavior on rotation {
            enabled: ShellState.folding
            NumberAnimation { duration: Theme.dur2; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
        }
    }
    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: eventPoint => root.menuRequested(root, eventPoint.position)
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
        textFormat: Text.PlainText
        text: root.name
        color: hover.hovered ? Theme.ink2 : Theme.ink3
        EaseColor on color {}
        font: Theme.type.silk
    }
}
