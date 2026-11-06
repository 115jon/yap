import QtQuick
import QtQuick.Templates as T
import com.yap.native

// The kit's keys (ui/button.tsx), squared 2 px corners and Bricolage 600:
// `primary` (primary-fill; at md and lg the C2 notch and a dithered cap),
// `secondary` (a 2 px line-strong edge), `ghost`, `destructive` (danger
// ink on danger tint), `danger` (danger fill) and `rule` (a bold label
// over a 2 px ink rule that turns accent on hover). Sizes 32 / 40 / 48.
// `loading` swaps the label for a spinner at the same width. Press scales
// to 0.97; disabled primary and danger go tonal, the rest fade to 45 %.
T.Button {
    id: control

    property string variant: "secondary"
    // sm | md | lg
    property string size: "md"
    property string iconName: ""
    // A glyph after the label (Browse collection's arrow).
    property string trailingIconName: ""
    property bool loading: false
    // The ply behind a notched primary key (the notch shows it).
    property color ground: Theme.ply0

    // `talk`: the call accept key (ui/dock.ts CALL_ACCEPT), talk green.
    readonly property bool filled: variant === "primary" || variant === "danger" || variant === "talk"
    readonly property bool notched: variant === "primary" && size !== "sm"
    readonly property bool tonal: filled && !enabled
    readonly property color ink: tonal ? Theme.ink3
        : variant === "primary" ? Theme.primaryFillInk
        : variant === "danger" ? Theme.dangerFillInk
        : variant === "talk" ? Theme.talkFillInk
        : variant === "destructive" ? Theme.dangerInk
        : Theme.ink1

    implicitHeight: variant === "rule" ? 40 : size === "sm" ? Theme.keySm : size === "lg" ? Theme.controlLg : Theme.control
    implicitWidth: implicitContentWidth + leftPadding + rightPadding
    leftPadding: variant === "rule" ? 4 : size === "sm" ? (iconName ? 10 : 12) : size === "lg" ? 20 : (iconName ? 14 : 16)
    rightPadding: notched ? 12 : leftPadding
    font: variant === "rule" ? Qt.font({ family: "Yap Bricolage Key Bold", pixelSize: 15, weight: Font.Bold })
        : size === "sm" ? Theme.type.controlSm : Theme.type.control
    opacity: enabled || filled ? 1 : 0.45
    scale: down ? Theme.scalePress : 1
    Behavior on scale { ScaleAnimator { duration: Theme.dur1; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }

    Accessible.role: Accessible.Button
    Accessible.name: text
    Keys.onReturnPressed: if (!loading) clicked()

    contentItem: Item {
        implicitWidth: row.implicitWidth
        implicitHeight: row.implicitHeight
        Row {
            id: row
            anchors.centerIn: parent
            spacing: control.notched ? 12 : control.size === "sm" ? 6 : 8
            opacity: control.loading ? 0 : 1
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                visible: control.iconName.length > 0
                name: control.iconName
                size: Theme.iconSm
                color: control.ink
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: control.text
                font: control.font
                color: control.ink
            }
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                visible: control.trailingIconName.length > 0
                name: control.trailingIconName
                size: Theme.iconSm
                color: control.ink
            }
            // The notched key's cap: DitherDots at 45 %, 12 x 20 (24 on lg).
            Image {
                anchors.verticalCenter: parent.verticalCenter
                visible: control.notched
                width: 12
                height: control.size === "lg" ? 24 : 20
                smooth: false
                source: control.notched ? "image://dither/dots/45/" + Theme.hex(control.ink) + "/6/" + (control.size === "lg" ? 12 : 10) : ""
            }
        }
        Icon {
            id: spinner
            anchors.centerIn: parent
            visible: control.loading
            name: "loader"
            size: Theme.iconSm
            color: control.ink
            smooth: true
            RotationAnimator on rotation {
                from: 0
                to: 360
                duration: 900
                loops: Animation.Infinite
                running: spinner.visible && !Theme.reducedMotion
            }
        }
    }

    background: Item {
        Rectangle {
            anchors.fill: parent
            visible: control.variant !== "rule" && control.variant !== "ghost" && control.variant !== "secondary"
            radius: Theme.radius
            color: control.tonal ? Theme.tint2
                : control.variant === "primary" ? Theme.primaryFill
                : control.variant === "danger" ? Theme.dangerFill
                : control.variant === "talk" ? Theme.talk
                : Theme.dangerTint
        }
        Rectangle {
            anchors.fill: parent
            visible: control.variant === "secondary"
            radius: Theme.radius
            color: "transparent"
            border.width: 2
            border.color: control.enabled ? Theme.lineStrong : Theme.line
        }
        Rectangle {
            anchors.fill: parent
            visible: control.variant !== "rule"
            radius: Theme.radius
            color: control.filled ? control.ink : Theme.ink1
            opacity: !control.enabled ? 0 : control.down ? 0.14 : control.hovered ? (control.filled ? 0.12 : 0.08) : 0
            Behavior on opacity { OpacityAnimator { duration: Theme.dur1 } }
        }
        Rectangle {
            visible: control.variant === "rule"
            anchors.bottom: parent.bottom
            width: parent.width
            height: 2
            color: !control.enabled ? Theme.lineStrong
                : control.hovered || control.down || control.visualFocus ? Theme.accentInk : Theme.ink1
        }
        // The notch: a square of the ground, turned 45 degrees on the corner.
        Rectangle {
            visible: control.notched
            x: parent.width - width / 2
            y: -height / 2
            width: 14.14
            height: 14.14
            rotation: 45
            antialiasing: true
            color: control.ground
        }
        Rectangle {
            anchors.fill: parent
            anchors.margins: -4
            visible: control.visualFocus && control.variant !== "rule"
            color: "transparent"
            radius: Theme.radius
            border.width: 2
            border.color: Theme.focus
        }
    }

    HoverHandler { cursorShape: Qt.PointingHandCursor }
}
