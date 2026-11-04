import QtQuick
import QtQuick.Templates as T
import com.yap.native

// An icon-only key (ui/IconButton.tsx and the dock recipe, ui/dock.ts).
//   tone "ghost"  ink-2, ink-1 on hover          (quiet rest)
//   tone "muted"  ink-3, ink-1 on hover          (header actions)
//   tone "rest"   tint-1, ink-1                  (dock rest)
//   tone "off"    muted-state tint and ink, the glyph's off partner
//   tone "toggled" tint-2, ink-1
// 24/28 keys take a 16 px glyph, 32 an 18, 40 a 20, 48 a 24.
T.Button {
    id: control

    required property string iconName
    property string tone: "ghost"
    property int size: 40
    property string iconVariant: tone === "off" ? "off" : "idle"
    property string tip: ""
    property bool square: false

    property int glyph: size >= 48 ? Theme.iconXl : size >= 40 ? Theme.iconLg : size >= 32 ? Theme.iconMd : Theme.iconSm
    readonly property color ink: tone === "off" ? Theme.mutedState
        : tone === "rest" || tone === "toggled" || hovered || down ? Theme.ink1
        : tone === "muted" ? Theme.ink3 : Theme.ink2

    implicitWidth: size
    implicitHeight: size
    padding: 0
    scale: down ? Theme.scalePressIcon : 1
    Behavior on scale { ScaleAnimator { duration: Theme.dur1; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }

    Accessible.role: Accessible.Button
    Accessible.name: tip

    contentItem: Item {
        Icon {
            anchors.centerIn: parent
            name: control.iconName
            variant: control.iconVariant
            size: control.glyph
            color: control.ink
        }
    }

    background: Rectangle {
        radius: control.square ? 0 : Theme.radius
        color: control.tone === "off" ? Theme.mutedStateTint
             : control.tone === "rest" ? Theme.tint1
             : control.tone === "toggled" ? Theme.tint2
             : "transparent"
        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            color: control.ink
            opacity: control.down ? 0.14 : control.hovered ? 0.08 : 0
            Behavior on opacity { OpacityAnimator { duration: Theme.dur1 } }
        }
        Rectangle {
            anchors.fill: parent
            anchors.margins: control.square ? 2 : -2
            visible: control.visualFocus
            color: "transparent"
            radius: Theme.radius
            border.width: 2
            border.color: Theme.focus
        }
    }
}
