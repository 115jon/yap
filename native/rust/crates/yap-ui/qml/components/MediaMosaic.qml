import QtQuick

// ImageGrid.tsx's mosaic for two or more pictures, shared by the desktop
// and the phone: a `gridWidth`-wide box with 4 px gaps, at most ten
// tiles, "+N" on the tenth. Each app draws its own tile through
// `delegate`: one per shown tile, given `index`, placed by itself at
// `rects[index]` (an id on this mosaic reaches it), with `more(index)` for
// the "+N" count. The tiles follow a width change without being made
// again.
Item {
    id: root

    property int count: 0
    property real gridWidth: 520
    property Component delegate: null

    readonly property var rects: gridRects(count, gridWidth)

    // The "+N" over the tenth tile: the pictures past the tenth.
    function more(index) {
        return index === 9 && count > 10 ? count - 10 : 0
    }

    // A tile's spoken name: what it opens, where it sits, and on the tenth
    // how many more there are ("+N").
    function tileName(name, index) {
        // Joined, not substituted after the name: a "%2" in a file name
        // stays as it is.
        const open = name ? qsTr("Open %1").arg(name) : qsTr("Open image")
        const where = qsTr("%1 of %2").arg(index + 1).arg(count)
        const extra = more(index)
        const rest = extra === 0 ? "" : extra === 1 ? qsTr("1 more image") : qsTr("%1 more images").arg(extra)
        return open + ", " + where + (rest.length > 0 ? ", " + rest : "")
    }

    // ImageGrid.tsx layouts as rectangles in a `w`-wide box.
    function gridRects(count, w) {
        const g = 4
        const half = (w - g) / 2
        const third = (w - 2 * g) / 3
        if (count === 2)
            return [{ x: 0, y: 0, w: half, h: w * 9 / 16 }, { x: half + g, y: 0, w: half, h: w * 9 / 16 }]
        if (count === 3) {
            const h = w * 2 / 3
            const cell = (h - g) / 2
            return [{ x: 0, y: 0, w: half, h: h }, { x: half + g, y: 0, w: half, h: cell },
                    { x: half + g, y: cell + g, w: half, h: cell }]
        }
        if (count === 4) {
            const cell = (w * 3 / 4 - g) / 2
            return [0, 1, 2, 3].map(i => ({ x: (i % 2) * (half + g), y: Math.floor(i / 2) * (cell + g), w: half, h: cell }))
        }
        if (count === 5) {
            const top = half * 9 / 16
            const out = [{ x: 0, y: 0, w: half, h: top }, { x: half + g, y: 0, w: half, h: top }]
            for (let i = 0; i < 3; ++i)
                out.push({ x: i * (third + g), y: top + g, w: third, h: third })
            return out
        }
        const shown = Math.min(count, 10)
        const out = [{ x: 0, y: 0, w: w, h: w / 2 }]
        for (let i = 1; i < shown; ++i) {
            const k = i - 1
            out.push({ x: (k % 3) * (third + g), y: w / 2 + g + Math.floor(k / 3) * (third + g), w: third, h: third })
        }
        return out
    }

    width: gridWidth
    height: rects.reduce((m, r) => Math.max(m, r.y + r.h), 0)

    // By count, not by the rectangles: a new width keeps the tiles.
    Repeater {
        model: root.rects.length
        delegate: root.delegate
    }
}
