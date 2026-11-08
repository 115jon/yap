import QtQuick
import com.yap.native

// The chosen tab's 2 px ink-1 rule (ui/tabs.tsx `rule`): one rule that
// slides and resizes to the chosen tab over 200 ms, ease-in-out, in place
// of a rule under each tab. Put it in the tabs' Row's parent, after the
// Row; the chosen tab is the Row child whose `flag` property is true:
//   Row { id: tabs; ... }
//   TabRule { row: tabs }              // children with `on`
//   TabRule { row: tabs; flag: "current" }
// The first placement does not slide, and neither does reduced motion
// (motion-reduce:transition-none).
Rectangle {
    id: rule

    property Item row: null
    property string flag: "on"
    readonly property Item tab: {
        if (!row)
            return null
        for (let i = 0; i < row.children.length; ++i) {
            const child = row.children[i]
            if (child.visible && child[rule.flag] === true)
                return child
        }
        return null
    }
    property bool ready: false

    visible: tab !== null
    x: row && tab ? row.x + tab.x : 0
    y: row ? row.y + row.height - height : 0
    width: tab ? tab.width : 0
    height: 2
    color: Theme.ink1

    Behavior on x {
        enabled: rule.ready && !Theme.reducedMotion
        NumberAnimation { duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeInOut }
    }
    Behavior on width {
        enabled: rule.ready && !Theme.reducedMotion
        NumberAnimation { duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeInOut }
    }
    // Armed once the Row has laid its tabs out.
    Timer {
        interval: 100
        running: true
        onTriggered: rule.ready = true
    }
}
