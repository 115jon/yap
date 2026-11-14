pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// The last ten as letters only (Deadlock.tsx Letters, M): ten W/L keys
// across the width, newest first.
Row {
    id: letters

    required property var dl
    readonly property var matches: dl.last10

    spacing: 3
    height: 18
    Accessible.role: Accessible.List
    Accessible.name: qsTr("Last %1 matches, newest first").arg(matches.length)

    Repeater {
        model: letters.matches
        delegate: BoardDeadlockChip {
            id: cell
            required property var modelData
            win: modelData.win
            width: Math.floor((letters.width - 27) / 10)
            height: 18
            Accessible.name: (modelData.win ? qsTr("Win") : qsTr("Loss")) + " " + qsTr("as %1").arg(letters.dl.heroOf(modelData.hero).name)
            HoverHandler { id: hover }
            Tooltip {
                visible: hover.hovered
                text: letters.dl.matchTip(cell.modelData)
            }
        }
    }
}
