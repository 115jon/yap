import QtQuick
import Yap.Ui

// A labelled field in a board widget's sheet: the label, the phone field
// and, while typing, how many characters are left. `text` is the draft's
// value; the field follows it except while it has focus (the store answers
// each key a moment later, and a stale answer must not overwrite what was
// typed since; undo still updates a field not being typed in). `edited`
// hands back what is typed with the word being composed, at the cursor.
Column {
    id: root

    property string label
    property string text: ""
    property int limit: 80
    property alias placeholderText: field.placeholderText
    property alias inputMethodHints: field.inputMethodHints
    property alias field: field
    readonly property string live: field.text.slice(0, field.cursorPosition) + field.preeditText
        + field.text.slice(field.cursorPosition)

    signal edited(string text)

    width: parent ? parent.width : 0
    spacing: 6

    Item {
        width: parent.width
        height: labelText.implicitHeight
        Text {
            id: labelText
            width: parent.width - left.width - 8
            textFormat: Text.PlainText
            text: root.label
            elide: Text.ElideRight
            color: Theme.ink1
            font: Theme.type.labelStrong
            Accessible.ignored: true
        }
        Text {
            id: left
            anchors.right: parent.right
            anchors.baseline: labelText.baseline
            visible: field.activeFocus
            textFormat: Text.PlainText
            text: String(Math.max(0, root.limit - root.live.length))
            color: root.live.length >= root.limit ? Theme.dangerInk : Theme.ink3
            font: Theme.type.numSm
            Accessible.ignored: true
        }
    }
    MobileField {
        id: field
        width: parent.width
        maximumLength: root.limit
        Accessible.name: root.label
        Binding on text {
            value: root.text
            when: !field.activeFocus
            restoreMode: Binding.RestoreNone
        }
        Component.onCompleted: text = root.text
    }
    onLiveChanged: if (field.activeFocus) edited(live)
}
