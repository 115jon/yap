import QtQuick
import com.yap.native

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

    spacing: 4

    Repeater {
        model: root.items
        delegate: Loader {
            id: slot
            required property var modelData
            required property int index
            readonly property bool picture: String(modelData.mime || "").startsWith("image/")
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
