import QtQuick
import QtQuick.Templates as T
import com.yap.native

// The settings switch (ui/switch.tsx, md): a 56 x 28 tint-3 track that
// fills with primary-fill when on, a talk LED on its leading end while on,
// and a 20 px squared thumb (ink-2, on-primary when on) that slides over
// dur-2. Disabled fades to 45 %. Named by its row (`Accessible.name`).
T.AbstractButton {
    id: control

    // Shows `checked`; the owner flips the setting on `clicked`.
    checkable: false
    implicitWidth: 56
    implicitHeight: 28
    focusPolicy: Qt.StrongFocus
    opacity: enabled ? 1 : 0.45
    scale: down ? Theme.scalePress : 1
    Behavior on scale { NumberAnimation { duration: Theme.dur1 } }

    Accessible.role: Accessible.CheckBox
    Accessible.checkable: true
    Accessible.checked: checked

    background: Rectangle {
        radius: Theme.radius
        color: Theme.tint3
        Rectangle {
            anchors.fill: parent
            radius: Theme.radius
            color: Theme.primaryFill
            opacity: control.checked ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: Theme.dur2 } }
        }
        Led {
            x: 8
            anchors.verticalCenter: parent.verticalCenter
            lit: control.checked
            color: Theme.talk
            opacity: control.checked ? 1 : 0
        }
        Rectangle {
            anchors.fill: parent
            anchors.margins: -4
            visible: control.visualFocus
            color: "transparent"
            radius: Theme.radius
            border.width: 2
            border.color: Theme.focus
        }
    }

    indicator: Rectangle {
        x: control.checked ? 32 : 4
        y: 4
        width: 20
        height: 20
        radius: 1
        color: control.checked ? Theme.primaryFillInk : Theme.ink2
        Behavior on x { NumberAnimation { duration: Theme.reducedMotion ? 0 : Theme.dur2; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
    }

    HoverHandler { cursorShape: Qt.PointingHandCursor }
}
