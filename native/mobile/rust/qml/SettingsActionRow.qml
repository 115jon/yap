import QtQuick
import Yap.Mobile
import Yap.Ui

// A Settings row that does one thing: opens a page of the app's own (the
// chevron), a page on the web (`url`, the external-link glyph) or acts
// (`iconName`, e.g. play). 48 high; the whole row is the key.
Item {
    id: row

    property string text
    property string url: ""
    property string iconName: url.length > 0 ? "externalLink" : "chevronRight"

    signal opened()

    function activate() {
        Native.haptic(Native.Tick)
        if (url.length > 0)
            Qt.openUrlExternally(url)
        else
            opened()
    }

    width: parent ? parent.width : 0
    height: 48
    Accessible.role: url.length > 0 ? Accessible.Link : Accessible.Button
    Accessible.name: text
    Accessible.onPressAction: row.activate()

    Rectangle {
        anchors.fill: parent
        anchors.leftMargin: -8
        anchors.rightMargin: -8
        radius: Theme.radius
        color: tap.pressed ? Theme.tint2 : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.dur1 } }
    }
    Text {
        anchors.left: parent.left
        anchors.right: glyph.left
        anchors.rightMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        textFormat: Text.PlainText
        text: row.text
        elide: Text.ElideRight
        color: Theme.ink1
        font: Theme.type.setting
    }
    Icon {
        id: glyph
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        name: row.iconName
        size: Theme.iconMd
        color: Theme.ink3
    }
    TapHandler {
        id: tap
        onTapped: row.activate()
    }
}
