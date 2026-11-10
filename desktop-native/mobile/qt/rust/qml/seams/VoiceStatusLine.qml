import QtQuick
import com.yap.native
import Yap.Ui

// A voice channel's status line under its row, mobile side: the text only
// (the desktop's adds clip previews through WebmPlayer).
Text {
    property string channelId: ""
    property string channelName: ""
    property string media: ""
    property bool editable: false

    color: Theme.ink3
    font: Theme.type.caption
    elide: Text.ElideRight
    visible: text.length > 0
}
