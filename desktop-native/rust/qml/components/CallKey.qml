import QtQuick
import com.yap.native

// A small mono key for the call panel; lit keys take the LED colour.
Rectangle {
    id: key

    required property string label
    property bool lit: false
    property color litColor: Theme.ink1
    signal clicked()

    height: 28
    width: keyText.implicitWidth + 24
    radius: Theme.radius
    color: lit ? litColor : keyHover.hovered ? Theme.ply4 : Theme.ply3

    Accessible.role: Accessible.Button
    Accessible.name: label

    Text {
        id: keyText
        anchors.centerIn: parent
        text: key.label
        color: key.lit ? Theme.ply0 : Theme.ink1
        font.family: Theme.mono
        font.pixelSize: 11
        font.weight: Font.DemiBold
        font.letterSpacing: 0.8
    }
    HoverHandler { id: keyHover; cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: key.clicked() }
}
