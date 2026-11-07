import QtQuick
import QtQuick.Templates as T
import com.yap.native

// The tooltip, C1 inverted (ui/tooltip.tsx): ink-1 fill, ply-0 text at
// 12/500, 28 high (more when it wraps), 4 x 8 padding, 2 px corners; 500
// ms delay, a 150 ms fade and scale from 0.97. Text only. Place inside the item it describes:
//   Tooltip { visible: hover.hovered; text: qsTr("…") }
// (The web's 300 ms instant hand-off between neighbours is not done yet.)
T.ToolTip {
    id: control

    // top (default), bottom (items at the top of a column), right (rail)
    // or left (a chip at a tile's right edge).
    property string side: "top"

    x: !parent ? 0 : side === "right" ? parent.width + 8 : side === "left" ? -implicitWidth - 8
        : Math.round((parent.width - implicitWidth) / 2)
    y: !parent ? 0 : side === "right" || side === "left" ? Math.round((parent.height - implicitHeight) / 2)
        : side === "bottom" ? parent.height + 6 : -implicitHeight - 6
    delay: 500
    margins: 8
    topPadding: 4
    bottomPadding: 4
    leftPadding: 8
    rightPadding: 8
    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(28, implicitContentHeight + topPadding + bottomPadding)
    closePolicy: T.Popup.NoAutoClose

    // max-w-[240px] with text-balance: a long tip wraps into lines of
    // about even length (the full date over two lines), not a full line
    // and a stray word.
    contentItem: Item {
        readonly property real full: Math.ceil(tipMetrics.advanceWidth)
        readonly property int lines: Math.ceil(full / (240 - 16))
        // The box takes the full 240; text-balance only evens the lines.
        implicitWidth: Math.min(240 - 16, full)
        implicitHeight: tipText.implicitHeight
        TextMetrics { id: tipMetrics; text: control.text; font: Theme.type.captionStrong }
        Text {
            id: tipText
            width: parent.lines > 1 ? Math.min(parent.width, Math.ceil(parent.full / parent.lines) + 12) : parent.width
            height: parent.height
            textFormat: Text.PlainText
            text: control.text
            wrapMode: Text.Wrap
            color: Theme.ply0
            font: Theme.type.captionStrong
            lineHeight: Theme.type.captionLine
            lineHeightMode: Text.FixedHeight
            verticalAlignment: Text.AlignVCenter
        }
    }
    background: Rectangle {
        // Presses pass through a tooltip (main.cpp PopupPressSink).
        objectName: "yapNoPressSink"
        radius: Theme.radius
        color: Theme.ink1
    }
    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.dur2; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
            NumberAnimation { property: "scale"; from: Theme.scaleIn; to: 1; duration: Theme.dur2; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
        }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: Theme.dur2Exit }
    }
}
