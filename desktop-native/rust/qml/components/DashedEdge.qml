import QtQuick

// A 1 px dashed edge (CSS `border border-dashed`): 3 px dashes 3 px apart,
// as Chromium draws a thin dashed border, around the item it fills. Put it
// over a Rectangle with no border of its own. Repaints when it resizes or
// its colour changes.
Canvas {
    id: edge

    property color color: "black"

    anchors.fill: parent
    Accessible.ignored: true
    onColorChanged: requestPaint()
    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()

    onPaint: {
        const ctx = getContext("2d")
        ctx.reset()
        ctx.strokeStyle = edge.color
        ctx.lineWidth = 1
        ctx.setLineDash([3, 3])
        ctx.strokeRect(0.5, 0.5, width - 1, height - 1)
    }
}
