import QtQuick
import com.yap.native
import Yap.Ui

// A multi-line text field on the kit's field edge (ui/textarea.tsx): ply-2,
// a 1 px line-field edge (ink-2 on hover, a 2 px ink-1 ring on focus),
// clamped to `max` characters. `value` is the owner's draft; `edited`
// reports a change.
Rectangle {
    id: root

    property string value
    property string placeholder
    property string accessibleName
    property int max: 1024

    signal edited(string value)

    // field-sizing-content with min-h-11: one line tall when empty, then
    // it grows with the text (to 240 here; the web's resize-y has no cap).
    implicitHeight: Math.min(240, Math.max(44, area.implicitHeight + 14))
    radius: Theme.radius
    color: Theme.ply2
    border.width: 1
    border.color: area.activeFocus ? Theme.ink1 : hover.hovered ? Theme.ink2 : Theme.lineField
    Behavior on border.color { ColorAnimation { duration: Theme.dur1 } }
    // styles.css :focus-visible: a focused input wears the 2 px --focus
    // outline outside its 1 px ink-1 edge (offset 0).
    Rectangle {
        anchors.fill: parent
        anchors.margins: -2
        visible: area.activeFocus
        radius: Theme.radius + 2
        color: "transparent"
        border.width: 2
        border.color: Theme.focus
    }

    HoverHandler { id: hover; cursorShape: Qt.IBeamCursor }
    // A press anywhere in the box puts the caret in it.
    TapHandler { onTapped: area.forceActiveFocus() }

    Flickable {
        id: flick
        // px-2.5 py-1.5 inside the 1 px edge.
        x: 11
        y: 7
        width: parent.width - 22
        height: parent.height - 14
        clip: true
        contentHeight: area.height
        boundsBehavior: Flickable.StopAtBounds
        TextEdit {
            id: area
            width: flick.width
            height: Math.max(implicitHeight, flick.height)
            wrapMode: TextEdit.Wrap
            color: Theme.ink1
            selectionColor: Theme.selection
            selectedTextColor: Theme.ink1
            // md:text-ui-row: 14/20.
            font.family: "Yap Geist"
            font.pixelSize: 14
            selectByMouse: true
            activeFocusOnTab: true
            Accessible.role: Accessible.EditableText
            Accessible.name: root.accessibleName
            // Follows the draft without a binding: the clamp assigns `text`.
            Component.onCompleted: text = root.value
            Connections {
                target: root
                function onValueChanged() {
                    if (area.text !== root.value)
                        area.text = root.value
                }
            }
            onTextChanged: {
                if (text.length > root.max) {
                    const at = cursorPosition
                    text = text.slice(0, root.max)
                    cursorPosition = Math.min(at, root.max)
                }
                if (text !== root.value)
                    root.edited(text)
            }
            onCursorRectangleChanged: {
                if (cursorRectangle.y < flick.contentY)
                    flick.contentY = cursorRectangle.y
                else if (cursorRectangle.y + cursorRectangle.height > flick.contentY + flick.height)
                    flick.contentY = cursorRectangle.y + cursorRectangle.height - flick.height
            }
            Text {
                visible: area.text.length === 0
                width: parent.width
                textFormat: Text.PlainText
                text: root.placeholder
                wrapMode: Text.Wrap
                color: Theme.ink3
                font: area.font
            }
        }
    }
}
