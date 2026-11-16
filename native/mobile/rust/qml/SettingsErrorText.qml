import QtQuick
import Yap.Ui

// A refusal under the Settings row it belongs to, while there is one.
Text {
    width: parent ? parent.width : 0
    visible: text.length > 0
    topPadding: 4
    textFormat: Text.PlainText
    wrapMode: Text.Wrap
    color: Theme.dangerInk
    font: Theme.type.caption
    Accessible.role: Accessible.AlertMessage
    Accessible.name: text
}
