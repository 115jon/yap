import QtQuick
import Yap.Ui

// A labelled field in Server settings: the label, the phone field, then
// its hint and error lines. Typing keeps the field above the keyboard
// (`page.reveal`). `text` is the draft's value: the field follows it
// without a binding, and `edited` hands back what is typed with the word
// being composed (`live`), which the field's own text must not take.
Column {
    id: root

    property string label
    property string hint: ""
    property string error: ""
    property string text: ""
    readonly property string live: field.text + field.preeditText
    property alias placeholderText: field.placeholderText
    property alias maximumLength: field.maximumLength
    property alias readOnly: field.readOnly
    property alias inputMethodHints: field.inputMethodHints
    property alias field: field
    property var page: null

    signal edited(string text)
    signal accepted()

    width: parent ? parent.width : 0

    onTextChanged: {
        if (live !== text)
            field.text = text
    }
    onLiveChanged: {
        if (live !== text)
            edited(live)
    }
    spacing: 6

    Text {
        textFormat: Text.PlainText
        text: root.label
        color: Theme.ink1
        font: Theme.type.labelStrong
        Accessible.ignored: true
    }
    MobileField {
        id: field
        width: parent.width
        invalid: root.error.length > 0
        Accessible.name: root.label
        Component.onCompleted: text = root.text
        onAccepted: root.accepted()
        onActiveFocusChanged: if (activeFocus && root.page) Qt.callLater(root.page.reveal, root)
    }
    AdminNote {
        error: false
        text: root.hint
        font: Theme.type.caption
    }
    AdminNote {
        text: root.error
    }
}
