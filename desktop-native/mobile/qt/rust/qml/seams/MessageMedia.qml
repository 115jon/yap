import QtQuick
import Yap.Ui

// A message's attachments, mobile side: images as pictures, the rest as a
// named row. The desktop's MessageMedia adds video, GIF streaming and the
// sensitive-media frame through desktop C++; that is the next step here.
Column {
    id: root

    required property var items
    property real maxWidth: 400
    property Item clipTo: null

    signal openImage(var images, int index)
    signal openVideo(var item)
    signal download(var item)
    signal expandVideo(var item)

    // The pictures, in order: a tap opens the viewer on them.
    function isPicture(item) {
        return String(item.mime || "").startsWith("image/")
    }
    readonly property var images: items.filter(isPicture)

    spacing: 4

    Repeater {
        model: root.items
        delegate: Loader {
            id: slot
            required property var modelData
            required property int index
            readonly property bool picture: root.isPicture(modelData)
            sourceComponent: picture ? image : file

            Component {
                id: image
                Image {
                    width: Math.min(root.maxWidth, 320)
                    height: status === Image.Ready ? Math.min(width * implicitHeight / Math.max(1, implicitWidth), 360) : width * 0.6
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                    source: slot.modelData.url || ""
                    sourceSize.width: width * 2
                    TapHandler {
                        onTapped: root.openImage(root.images, root.items.slice(0, slot.index).filter(root.isPicture).length)
                    }
                }
            }
            Component {
                id: file
                Rectangle {
                    width: Math.min(root.maxWidth, 320)
                    height: 44
                    radius: Theme.radius
                    color: Theme.ply1
                    border.color: Theme.line
                    Text {
                        anchors.fill: parent
                        anchors.margins: 12
                        verticalAlignment: Text.AlignVCenter
                        text: slot.modelData.name || "File"
                        color: Theme.ink1
                        font: Theme.type.row
                        elide: Text.ElideMiddle
                    }
                    TapHandler {
                        onTapped: root.download(slot.modelData)
                    }
                }
            }
        }
    }
}
