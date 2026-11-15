import QtQuick
import Yap.Ui

// A line under a field or a key in Server settings: danger ink for an
// error (`error`, announced), else ink-3. Hidden while empty.
Text {
    property bool error: true

    width: parent ? parent.width : 0
    visible: text.length > 0
    textFormat: Text.PlainText
    wrapMode: Text.Wrap
    color: error ? Theme.dangerInk : Theme.ink3
    font: error ? Theme.type.caption : Theme.type.description
    Accessible.role: error ? Accessible.AlertMessage : Accessible.StaticText
    Accessible.name: text
}
