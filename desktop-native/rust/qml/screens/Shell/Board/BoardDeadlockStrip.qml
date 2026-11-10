pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// The last ten as hero faces (Deadlock.tsx Strip, L): five across, a win a
// full face over a solid W, a loss a dimmed face over an outlined L.
Grid {
    id: strip

    required property var dl

    columns: 5
    columnSpacing: 6
    rowSpacing: 6
    Accessible.role: Accessible.List
    Accessible.name: qsTr("Last %1 matches, newest first").arg(dl.last10.length)

    Repeater {
        model: strip.dl.last10
        delegate: Column {
            id: cell
            required property var modelData
            readonly property var hero: strip.dl.heroOf(modelData.hero)
            spacing: 3
            Accessible.role: Accessible.ListItem
            Accessible.name: qsTr("%1 as %2, %3").arg(modelData.win ? qsTr("Win") : qsTr("Loss"))
                .arg(hero.name).arg(modelData.k + "/" + modelData.d + "/" + modelData.a)

            BoardDeadlockArt {
                width: 28
                height: 28
                opacity: cell.modelData.win ? 1 : 0.55
                source: strip.dl.heroArt(cell.hero, "sm")
                initial: cell.hero.name
            }
            BoardDeadlockChip {
                win: cell.modelData.win
                Accessible.ignored: true
            }
            HoverHandler { id: hover }
            Tooltip {
                visible: hover.hovered
                text: strip.dl.matchTip(cell.modelData)
            }
        }
    }
}
