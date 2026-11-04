import QtQuick
import QtQuick.Templates as T
import com.yap.native

// The tooltip, C1 inverted (ui/tooltip.tsx): ink-1 fill, ply-0 text at
// 12/500, 28 high, 4 x 8 padding, 2 px corners; 500 ms delay, a 150 ms fade
// and scale from 0.97. Text only. Place inside the item it describes:
//   Tooltip { visible: hover.hovered; text: qsTr("…") }
// (The web's 300 ms instant hand-off between neighbours is not done yet.)
T.ToolTip {
    id: control

    // top (default), bottom (items at the top of a column) or right (rail).
    property string side: "top"

    x: !parent ? 0 : side === "right" ? parent.width + 8 : Math.round((parent.width - implicitWidth) / 2)
    y: !parent ? 0 : side === "right" ? Math.round((parent.height - implicitHeight) / 2)
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

    contentItem: Text {
        text: control.text
        color: Theme.ply0
        font: Theme.type.captionStrong
        verticalAlignment: Text.AlignVCenter
    }
    background: Rectangle {
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
