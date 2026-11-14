pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// One order of a profile frame (ProfileFrameLayer.tsx, lib/profile-frame.ts)
// filling the frame box: the card plus the footprint the frame reserves
// around it (`side`, `top` and `bottom`, in fractions of the box width).
// Staples keep their aspect, pinned to the top or bottom edge; rails run
// the whole height; a border rings the card itself.
Item {
    id: root

    // The store's frame JSON: `{side, top, bottom, layers: [{src, type,
    // order, anchor}]}`, or empty.
    property string frame: ""
    property string order: "front"

    readonly property var info: frame.length > 0 ? JSON.parse(frame) : null

    Accessible.ignored: true

    Repeater {
        model: root.info === null ? [] : root.info.layers.filter(l => l.order === root.order)
        Image {
            id: art
            required property var modelData
            readonly property real topInset: root.info.top * root.width
            readonly property real bottomInset: root.info.bottom * root.width
            readonly property real natural: status === Image.Ready && implicitWidth > 0
                ? root.width * implicitHeight / implicitWidth : 0
            width: root.width
            height: modelData.type === "rail" ? root.height
                : modelData.type === "border" ? root.height - topInset - bottomInset
                : natural
            y: modelData.type === "rail" ? 0
                : modelData.type === "border" ? topInset
                : modelData.anchor === "bottom" ? root.height - height : 0
            source: modelData.src
            fillMode: Image.Stretch
            asynchronous: true
            sourceSize.width: Math.ceil(root.width * 2)
        }
    }
}
