pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// Photo strip (widgets/Photos.tsx): one to four of the owner's pictures,
// cropped into fixed boxes so the plate never changes shape, with an
// optional caption. GIFs hold still until hovered (WidgetImage).
// S: one picture. M: stacked (one or two) or a 2 x 2 grid (three: the
// first across the top). L: side by side.
WidgetPlate {
    id: root

    required property var widget
    required property var boardData
    required property var ctx

    readonly property var photos: widget.cfg?.photos ?? []
    readonly property var shown: photos.slice(0, size === "S" ? 1 : 4)
    readonly property string caption: widget.cfg?.caption ?? ""
    readonly property bool showCaption: caption.length > 0 && size !== "S" && shown.length > 0
    readonly property int gap: 4

    kind: widget.type
    size: widget.size
    draft: widget.draft ?? false
    readout: photos.length > 0 ? String(photos.length) : ""

    // The box for photo `i` of `n` in a `w` x `h` area.
    function cellRect(i, n, w, h) {
        const g = gap
        if (size === "L") {
            const cw = (w - g * (n - 1)) / n
            return { x: i * (cw + g), y: 0, w: cw, h: h }
        }
        if (size === "M" && n > 2) {
            const cw = (w - g) / 2
            const ch = (h - g) / 2
            if (n === 3) {
                if (i === 0)
                    return { x: 0, y: 0, w: w, h: ch }
                return { x: (i - 1) * (cw + g), y: ch + g, w: cw, h: ch }
            }
            return { x: (i % 2) * (cw + g), y: Math.floor(i / 2) * (ch + g), w: cw, h: ch }
        }
        const rh = (h - g * (n - 1)) / n
        return { x: 0, y: i * (rh + g), w: w, h: rh }
    }

    WidgetEmpty {
        visible: root.shown.length === 0
        icon: "image"
        title: root.ctx?.isOwner ? qsTr("No photos yet") : qsTr("No photos")
        hint: root.ctx?.isOwner && root.size !== "S" ? qsTr("Add up to 4 in Edit board.") : ""
    }

    Item {
        id: strip
        visible: root.shown.length > 0
        width: parent.width
        height: parent.height - (root.showCaption ? captionText.height + 10 : 0)
        Accessible.role: Accessible.List
        Accessible.name: qsTr("Photos")

        Repeater {
            model: root.shown
            delegate: WidgetImage {
                id: photo
                required property var modelData
                required property int index
                readonly property var box: root.cellRect(index, root.shown.length, strip.width, strip.height)
                x: Math.round(box.x)
                y: Math.round(box.y)
                width: Math.round(box.x + box.w) - x
                height: Math.round(box.y + box.h) - y
                source: modelData.url ?? ""
                alt: modelData.alt ?? ""
                decodeWidth: Math.max(160, Math.min(960, Math.ceil(width * 2)))
            }
        }
    }

    Text {
        id: captionText
        visible: root.showCaption
        anchors.bottom: parent.bottom
        width: parent.width
        textFormat: Text.PlainText
        text: root.caption
        wrapMode: Text.Wrap
        maximumLineCount: 2
        elide: Text.ElideRight
        color: Theme.ink3
        font: Theme.type.caption
    }
}
