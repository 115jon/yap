import QtQuick
import com.yap.native
import Yap.Ui

// "Playing **Name**" on a row's second line, in the custom status's place
// (GameActivity.tsx GameActivityText), after the game's 16 px catalog icon
// from /api/game-icons. Without an icon (none in the catalog, still
// loading, or failed) the line is the words alone, never a stand-in glyph.
// Rows show no clock, so nothing ticks.
Row {
    id: root

    required property string game
    property string icon: ""
    property font font: Theme.type.caption
    property color ink: Theme.ink3
    property color strongInk: Theme.ink2

    readonly property bool hasIcon: picture.status === Image.Ready
    readonly property real iconRoom: hasIcon ? 16 + spacing : 0

    height: 16
    spacing: 4
    Accessible.role: Accessible.StaticText
    Accessible.name: qsTr("Playing %1").arg(game)

    Item {
        visible: root.hasIcon
        width: 16
        height: 16
        anchors.verticalCenter: parent.verticalCenter
        Accessible.ignored: true
        Image {
            id: picture
            anchors.fill: parent
            source: root.icon
            sourceSize.width: 32
            sourceSize.height: 32
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            cache: true
        }
    }
    // "Playing" keeps its width; the name elides ("Playing Overw…").
    Text {
        id: lead
        anchors.verticalCenter: parent.verticalCenter
        textFormat: Text.PlainText
        text: qsTr("Playing")
        color: root.ink
        font: root.font
    }
    Text {
        anchors.verticalCenter: parent.verticalCenter
        visible: width > 0
        width: Math.max(0, Math.min(implicitWidth, root.width - root.iconRoom - lead.implicitWidth - root.spacing))
        textFormat: Text.PlainText
        text: root.game
        elide: Text.ElideRight
        color: root.strongInk
        font: Qt.font({ family: "Yap Geist SemiBold", pixelSize: root.font.pixelSize, weight: Font.DemiBold, hintingPreference: root.font.hintingPreference })
    }
}
