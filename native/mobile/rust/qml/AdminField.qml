import QtQuick
import Yap.Ui

// A labelled field in Server settings: the label, the phone field, then
// its hint and error lines. Typing keeps the field above the keyboard
// (`page.reveal`).
Column {
    id: root

    property string label
    property string hint: ""
    property string error: ""
    property alias text: field.text
    property alias placeholderText: field.placeholderText
    property alias maximumLength: field.maximumLength
    property alias readOnly: field.readOnly
    property alias inputMethodHints: field.inputMethodHints
    property alias field: field
    property var page: null

    signal edited(string text)
    signal accepted()

    width: parent ? parent.width : 0
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
        onTextEdited: root.edited(text)
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
