pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// Last match, M (Deadlock.tsx LastMatchMedium): the hero's art band with
// the result and mode, the hero and the K/D/A big, up to six numbers
// (souls, last hits, rank points or mode, damage, level) and the final
// build at the foot.
Item {
    id: view

    required property var dl
    readonly property var match: dl.lastMatch
    readonly property var hero: dl.heroOf(match.hero)
    readonly property string mode: dl.modeLabel(match.mode)
    readonly property var cells: {
        const m = match
        const k = v => v < 1000 ? { value: String(Math.round(v)), unit: "" } : { value: (v / 1000).toFixed(1), unit: "k" }
        const out = []
        out.push(Object.assign({ label: qsTr("Souls") }, k(m.souls)))
        out.push({ label: qsTr("Last hits"), value: String(m.last_hits), unit: "" })
        if (m.delta !== null && m.delta !== undefined)
            out.push({ label: qsTr("Rank"), value: dl.signed(m.delta), unit: "" })
        else
            out.push({ label: qsTr("Mode"), value: mode, unit: "" })
        if (m.hero_damage !== null && m.hero_damage !== undefined)
            out.push(Object.assign({ label: qsTr("Hero dmg") }, k(m.hero_damage)))
        if (m.objective_damage !== null && m.objective_damage !== undefined)
            out.push(Object.assign({ label: qsTr("Obj. dmg") }, k(m.objective_damage)))
        if (m.level !== null && m.level !== undefined)
            out.push({ label: qsTr("Level"), value: String(m.level), unit: "" })
        return out
    }

    Column {
        id: upper
        width: view.width
        spacing: 10

        BoardDeadlockBand {
            dl: view.dl
            hero: view.hero
            width: parent.width
            height: 120
            tag: (view.match.win ? qsTr("Win") : qsTr("Loss")) + " · " + view.mode
        }

        Item {
            width: parent.width
            height: Math.max(names.height, kda.height)
            Column {
                id: names
                anchors.bottom: parent.bottom
                width: parent.width - kda.width - 8
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: view.hero.name
                    elide: Text.ElideRight
                    color: Theme.ink1
                    font: view.dl.titleFont
                    Accessible.role: Accessible.Heading
                    Accessible.name: text
                }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: view.dl.clockText(view.match.duration) + " · "
                        + (view.match.badge ? view.dl.rankName(view.match.badge) : view.mode)
                    elide: Text.ElideRight
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
            Text {
                id: kda
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                height: 30
                verticalAlignment: Text.AlignVCenter
                textFormat: Text.RichText
                text: view.match.k + "<span style='color:" + Theme.ink3 + ";font-weight:400'>/</span>"
                    + view.match.d + "<span style='color:" + Theme.ink3 + ";font-weight:400'>/</span>" + view.match.a
                color: Theme.ink1
                font: view.dl.bigFont
                Accessible.role: Accessible.StaticText
                Accessible.name: qsTr("%1 kills, %2 deaths, %3 assists").arg(view.match.k).arg(view.match.d).arg(view.match.a)
            }
        }

        Grid {
            id: grid
            width: parent.width
            columns: 3
            columnSpacing: 8
            rowSpacing: 8
            readonly property real cell: (width - 16) / 3
            Repeater {
                model: view.cells
                delegate: BoardDeadlockStat {
                    required property var modelData
                    dl: view.dl
                    width: grid.cell
                    label: modelData.label
                    value: modelData.value
                    unit: modelData.unit
                }
            }
        }
    }

    Column {
        visible: (view.match.items ?? []).length > 0
        width: view.width
        y: Math.max(upper.height + 10, view.height - height)
        spacing: 6
        Text {
            textFormat: Text.PlainText
            text: qsTr("Final build")
            color: Theme.ink3
            font: Theme.type.silk
        }
        Row {
            spacing: 4
            Accessible.role: Accessible.List
            Accessible.name: qsTr("Final build")
            Repeater {
                model: view.match.items ?? []
                delegate: BoardDeadlockArt {
                    id: item
                    required property var modelData
                    readonly property string name: view.dl.digest?.items?.[String(modelData)]?.name ?? qsTr("Item")
                    width: 36
                    height: 36
                    source: view.dl.itemArt(modelData)
                    alt: name
                    HoverHandler { id: hover }
                    Tooltip {
                        visible: hover.hovered
                        text: item.name
                    }
                }
            }
        }
    }
}
