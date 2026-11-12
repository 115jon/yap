import QtQuick
import Yap.Ui

// A nameplate behind a row, mobile side: the still frame, cropped and
// anchored right where the art is. The desktop's NameplateLayer also loops
// the plate's webm over it while hovered (WebmPlayer, desktop-only C++);
// nothing hovers here.
Item {
    id: root

    property string plate: ""
    property bool playing: false
    property bool identityFade: false
    property color ground: Theme.ply1

    readonly property var info: plate.length > 0 ? JSON.parse(plate) : null

    clip: true
    Accessible.ignored: true

    Image {
        anchors.fill: parent
        visible: root.info !== null && root.info.still.length > 0
        source: visible ? root.info.still : ""
        fillMode: Image.PreserveAspectCrop
        horizontalAlignment: Image.AlignRight
        asynchronous: true
        sourceSize.height: Math.ceil(root.height * 2)
        opacity: 0.98
    }
}
