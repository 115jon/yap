import QtQuick
import com.yap.native

// "Playing **Name**" on a row's second line, in the custom status's place
// (GameActivity.tsx GameActivityText), after the game's 16 px icon: the
// catalog icon from /api/game-icons, the gamepad glyph while it loads, when
// there is none or when it fails. Rows show no clock, so nothing ticks.
Row {
    id: root

    required property string game
    property string icon: ""
    property font font: Theme.type.caption
    property color ink: Theme.ink3
    property color strongInk: Theme.ink2

    height: 16
    spacing: 4
    Accessible.role: Accessible.StaticText
    Accessible.name: qsTr("Playing %1").arg(game)

    Item {
        width: 16
        height: 16
        anchors.verticalCenter: parent.verticalCenter
        Accessible.ignored: true
        Image {
            id: picture
            anchors.fill: parent
            visible: status === Image.Ready
            source: root.icon
            sourceSize.width: 32
            sourceSize.height: 32
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            cache: true
        }
        Icon {
            anchors.centerIn: parent
            visible: picture.status !== Image.Ready
            name: "gamepad"
            size: 14
            color: root.ink
        }
    }
    // "Playing" then the name in the semibold cut. The name elides first;
    // in a very narrow row the word elides too.
    Text {
        id: lead
        anchors.verticalCenter: parent.verticalCenter
        width: Math.max(0, Math.min(implicitWidth, root.width - 16 - root.spacing))
        textFormat: Text.PlainText
        text: qsTr("Playing")
        elide: Text.ElideRight
        color: root.ink
        font: root.font
    }
    Text {
        anchors.verticalCenter: parent.verticalCenter
        visible: width > 0
        width: Math.max(0, Math.min(implicitWidth, root.width - 16 - lead.width - 2 * root.spacing))
        textFormat: Text.PlainText
        text: root.game
        elide: Text.ElideRight
        color: root.strongInk
        font: Qt.font({ family: "Yap Geist SemiBold", pixelSize: root.font.pixelSize, weight: Font.DemiBold, hintingPreference: root.font.hintingPreference })
    }
}
