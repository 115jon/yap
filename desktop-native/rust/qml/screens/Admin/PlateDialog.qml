import QtQuick
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// The kit's dialog plate (ui/dialog.tsx, responsive-dialog.tsx at md, top
// placement): ply-3 with a line-strong edge, 24 padding; an optional
// silkscreen kicker, the title (Bricolage 20), an optional dithered rule,
// the description, then the fields and a right-aligned footer of keys.
// Opens with scale 0.96 and a fade; Escape or a press outside closes it
// unless it is busy. `c1` is the web's DialogContent as it stands: centred
// (placement center), the 28/32 title, the 15/22 description, 16 between
// the header and the body, the 40 px tint-2 close key 16 in from the top
// right, and no footer gap without footer keys.
T.Popup {
    id: dialog

    property string kicker
    property string title
    property string description
    property bool rule: false
    property bool busy: false
    property bool c1: false
    // A C1 content or form dialog (DialogContent placement "top"): its top
    // at max(48px, 12vh) of the window, not centred.
    property bool placeTop: false
    default property alias content: fields.data
    property alias footer: footerRow.data

    parent: T.Overlay.overlay
    x: Math.round((parent.width - width) / 2)
    y: c1 && placeTop ? Math.max(48, Math.round(parent.height * 0.12))
        : c1 ? Math.max(Theme.chrome + 16, Math.round((parent.height - height) / 2 + Theme.chrome / 2))
        : Math.max(Theme.chrome + 32, Math.round(parent.height * 0.14))
    width: Math.min(480, parent.width - 32)
    // Templates popups have no implicit size of their own.
    implicitHeight: implicitContentHeight + topPadding + bottomPadding
    // C1: p-6 inside the 1 px edge.
    padding: c1 ? 25 : 24
    modal: true
    // Ctrl+K stays shut while a modal is up (ExtrasHost).
    onVisibleChanged: ShellState.modalCount += visible ? 1 : -1
    focus: true
    closePolicy: busy ? T.Popup.NoAutoClose : T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutside

    T.Overlay.modal: Scrim { color: Theme.scrim }

    enter: PresenceIn { fromScale: Theme.scaleDialog }
    exit: PresenceOut { toScale: Theme.scaleDialogOut }

    background: Rectangle {
        radius: Theme.radiusSheet
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong

        // The close key (DialogPrimitive.Close): over the plate, not in
        // the column, as the web's sits absolute at top-4 right-4.
        IconKey {
            visible: dialog.c1
            // top-4 right-4 inside the 1 px edge.
            x: parent.width - width - 17
            y: 17
            size: 40
            glyph: Theme.iconMd
            tone: "toggled"
            iconVariant: "idle"
            iconName: "close"
            tip: qsTr("Close")
            enabled: !dialog.busy
            onClicked: dialog.close()
        }
    }

    contentItem: Column {
        spacing: 0
        Accessible.role: Accessible.Dialog
        Accessible.name: dialog.title

        Text {
            visible: dialog.kicker.length > 0
            width: parent.width
            bottomPadding: 8
            textFormat: Text.PlainText
            text: dialog.kicker
            elide: Text.ElideRight
            color: Theme.ink3
            font: Theme.type.silk
        }
        Text {
            width: parent.width
            // pr-12: clear of the close key.
            rightPadding: dialog.c1 ? 48 : 0
            textFormat: Text.PlainText
            text: dialog.title
            wrapMode: Text.Wrap
            color: Theme.ink1
            font: dialog.c1 ? Theme.type.dialogTitle : Theme.type.sectionTitle
            lineHeight: dialog.c1 ? Theme.type.dialogTitleLine : 1
            lineHeightMode: dialog.c1 ? Text.FixedHeight : Text.ProportionalHeight
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
            textFormat: Text.PlainText
            rightPadding: dialog.c1 ? 48 : 0
            text: dialog.description
            wrapMode: Text.Wrap
            color: Theme.ink2
            font: dialog.c1 ? Theme.type.dialogBody : Theme.type.description
            lineHeight: dialog.c1 ? Theme.type.dialogBodyLine : Theme.type.descriptionLine
            lineHeightMode: Text.FixedHeight
        }
        Item { width: 1; height: fields.height > 0 ? (dialog.c1 ? 16 : 20) : 0 }
        Column {
            id: fields
            width: parent.width
            spacing: 16
        }
        // A C1 plate without footer keys ends at its body (no footer).
        Item { width: 1; height: dialog.c1 && !footerRow.shown ? 0 : 24 }
        Row {
            id: footerRow
            // Any footer key showing (keys hidden by `visible: false` still
            // gave the row its height, so a key-less C1 plate kept a 64 px
            // tail).
            readonly property bool shown: {
                for (let i = 0; i < children.length; ++i)
                    if (children[i].visible)
                        return true
                return false
            }
            height: !dialog.c1 || shown ? implicitHeight : 0
            anchors.right: parent.right
            spacing: 8
        }
    }
}
