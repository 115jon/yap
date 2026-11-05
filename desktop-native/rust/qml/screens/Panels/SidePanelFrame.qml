import QtQuick
import com.yap.native

// A right-hand side panel (ui/side-panel.tsx): a 48 px header (px-4, pr-2,
// the title at ui-row 14/600, then any adornment, the actions and a 32 px
// close key) over a body that owns its scroller. The shell docks it as a
// 360 px ply-1 column at 1024 px and up, or shows it in a right sheet.
Item {
    id: root

    property string title
    // Shown after the title (a count).
    property alias adornment: adornmentSlot.data
    // Trailing header keys, before Close.
    property alias actions: actionsSlot.data
    // The body, under the header.
    default property alias content: body.data

    signal closeRequested()

    Accessible.role: Accessible.Pane
    Accessible.name: title

    Item {
        id: header
        width: parent.width
        height: Theme.header
        Text {
            id: titleText
            x: 16
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, header.width - 16 - tail.width - 8 - adornmentSlot.width)
            textFormat: Text.PlainText
            text: root.title
            elide: Text.ElideRight
            color: Theme.ink1
            font: Theme.type.panelTitle
            Accessible.role: Accessible.Heading
        }
        Row {
            id: adornmentSlot
            anchors.left: titleText.right
            anchors.leftMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
        }
        Row {
            id: tail
            anchors.right: parent.right
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            Row {
                id: actionsSlot
                anchors.verticalCenter: parent.verticalCenter
                spacing: 4
            }
            IconKey {
                anchors.verticalCenter: parent.verticalCenter
                iconName: "close"
                size: 32
                tone: "muted"
                tip: qsTr("Close")
                tipSide: "bottom"
                onClicked: root.closeRequested()
            }
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
    }

    Item {
        id: body
        anchors.top: header.bottom
        anchors.bottom: parent.bottom
        width: parent.width
    }
}
