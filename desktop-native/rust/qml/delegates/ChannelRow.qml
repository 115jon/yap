import QtQuick
import QtQuick.Shapes
import com.yap.native

// A text or voice channel in the tree.
Rectangle {
    id: row

    required property string name
    required property bool voice
    required property bool active
    signal selected()

    height: Theme.treeRow
    radius: Theme.radius
    color: active ? Theme.ply3 : hover.hovered ? Theme.ply2 : "transparent"

    Text {
        visible: !row.voice
        anchors.left: parent.left
        anchors.leftMargin: 10
        anchors.verticalCenter: parent.verticalCenter
        text: "#"
        color: row.active ? Theme.ink2 : Theme.inkOff
        font.family: Theme.mono
        font.pixelSize: 15
    }
    // Speaker glyph for voice channels.
    Shape {
        visible: row.voice
        x: 8
        anchors.verticalCenter: parent.verticalCenter
        width: 14
        height: 14
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeWidth: 1.4
            strokeColor: row.active ? Theme.ink2 : Theme.inkOff
            fillColor: "transparent"
            joinStyle: ShapePath.RoundJoin
            capStyle: ShapePath.RoundCap
            PathSvg { path: "M2 5 H5 L9 2 V12 L5 9 H2 Z M11.5 4.5 Q13.5 7 11.5 9.5" }
        }
    }
    Text {
        anchors.left: parent.left
        anchors.leftMargin: 28
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        text: row.name
        elide: Text.ElideRight
        color: row.active ? Theme.ink1 : hover.hovered ? Theme.ink2 : Theme.ink3
        font.family: Theme.text
        font.pixelSize: 14
        font.weight: Font.Medium
    }
    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: row.selected() }
}
