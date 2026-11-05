import QtQuick
import QtQuick.Templates as T
import com.yap.native

// The kit's dialog plate (ui/dialog.tsx, responsive-dialog.tsx at md, top
// placement): ply-3 with a line-strong edge, 24 padding; an optional
// silkscreen kicker, the title (Bricolage 20), an optional dithered rule,
// the description, then the fields and a right-aligned footer of keys.
// Opens with scale 0.96 and a fade; Escape or a press outside closes it
// unless it is busy.
T.Popup {
    id: dialog

    property string kicker
    property string title
    property string description
    property bool rule: false
    property bool busy: false
    default property alias content: fields.data
    property alias footer: footerRow.data

    parent: T.Overlay.overlay
    x: Math.round((parent.width - width) / 2)
    y: Math.max(Theme.chrome + 32, Math.round(parent.height * 0.14))
    width: Math.min(480, parent.width - 32)
    // Templates popups have no implicit size of their own.
    implicitHeight: implicitContentHeight + topPadding + bottomPadding
    padding: 24
    modal: true
    focus: true
    closePolicy: busy ? T.Popup.NoAutoClose : T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutside

    T.Overlay.modal: Rectangle { color: Theme.scrim }

    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
            NumberAnimation { property: "scale"; from: Theme.scaleDialog; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
        }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: Theme.dur3Exit }
    }

    background: Rectangle {
        radius: Theme.radiusSheet
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
    }

    contentItem: Column {
        spacing: 0
        Accessible.role: Accessible.Dialog
        Accessible.name: dialog.title

        Text {
            visible: dialog.kicker.length > 0
            width: parent.width
            bottomPadding: 8
            text: dialog.kicker
            elide: Text.ElideRight
            color: Theme.ink3
            font: Theme.type.silk
        }
        Text {
            width: parent.width
            text: dialog.title
            wrapMode: Text.Wrap
            color: Theme.ink1
            font: Theme.type.sectionTitle
            Accessible.role: Accessible.Heading
        }
        Item {
            visible: dialog.rule
            width: parent.width
            height: 16 + 4
            DitherRule {
                y: 12
                width: parent.width
            }
        }
        Text {
            visible: dialog.description.length > 0
            width: parent.width
            topPadding: 8
            text: dialog.description
            wrapMode: Text.Wrap
            color: Theme.ink2
            font: Theme.type.description
            lineHeight: Theme.type.descriptionLine
            lineHeightMode: Text.FixedHeight
        }
        Item { width: 1; height: fields.height > 0 ? 20 : 0 }
        Column {
            id: fields
            width: parent.width
            spacing: 16
        }
        Item { width: 1; height: 24 }
        Row {
            id: footerRow
            anchors.right: parent.right
            spacing: 8
        }
    }
}
