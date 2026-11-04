import QtQuick
import QtQuick.Templates as T
import com.yap.native

// The menu panel (ui/menu.ts): ply-3 with a 1 px line-strong edge and a
// 2 px ink-1 top rule, 4 px padding, 200-280 wide. Rows are MenuRow. Opens
// with scale 0.97 and a fade over 200 ms (exit 140).
T.Menu {
    id: control

    implicitWidth: Math.min(280, Math.max(200, contentItem.implicitWidth + leftPadding + rightPadding))
    implicitHeight: contentItem.implicitHeight + topPadding + bottomPadding
    padding: 4
    topPadding: 6
    margins: 8
    overlap: 0

    delegate: MenuRow {}

    contentItem: ListView {
        implicitHeight: contentHeight
        implicitWidth: 200
        model: control.contentModel
        interactive: false
        clip: false
        currentIndex: control.currentIndex
    }

    background: Rectangle {
        implicitWidth: 200
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        Rectangle {
            width: parent.width
            height: 2
            color: Theme.ink1
        }
    }

    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
            NumberAnimation { property: "scale"; from: Theme.scaleIn; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
        }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: Theme.dur3Exit }
    }
}
