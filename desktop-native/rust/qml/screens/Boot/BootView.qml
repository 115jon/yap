import QtQuick
import com.yap.native

// The cold load (chat/ColdLoad.tsx, wider screens): a compact plate in the
// middle of the window, Yappy barking while the steps run over a three-cell
// step meter, a silkscreen kicker and one Bricolage line that changes as the
// steps complete. The bark is the only motion, and only while loading.
Item {
    id: root

    required property string status
    // Only animate while shown.
    required property bool active

    // 0 signing in, 1 opening chat, 2 loading messages.
    readonly property int step: status.indexOf("servers") >= 0 ? 2 : 0
    readonly property var lines: [
        { line: qsTr("Signing you in"), detail: qsTr("Checking your session on this device.") },
        { line: qsTr("Opening chat"), detail: qsTr("Connecting to the chat server.") },
        { line: qsTr("Loading messages"), detail: qsTr("Fetching your servers and what you missed.") }
    ]

    Rectangle {
        id: plate
        anchors.horizontalCenter: parent.horizontalCenter
        y: Math.max(24, parent.height / 2 - 190)
        width: 400
        height: content.implicitHeight + 64
        radius: Theme.radius
        color: Theme.ply0
        border.width: 1
        border.color: Theme.lineStrong
        Rectangle { width: parent.width; height: 2; color: Theme.ink1 }

        Column {
            id: content
            x: 32
            y: 32
            width: parent.width - 64
            spacing: 0

            YapBark {
                anchors.horizontalCenter: parent.horizontalCenter
                cell: 6
                barking: root.active
            }
            Row {
                topPadding: 32
                spacing: 12
                Row {
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4
                    Repeater {
                        model: 3
                        delegate: Rectangle {
                            required property int index
                            width: 24
                            height: 2
                            color: index < root.step ? Theme.ink1 : index === root.step ? Theme.ink3 : Theme.lineStrong
                        }
                    }
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: "0" + (root.step + 1) + " / 03"
                    color: Theme.ink3
                    font: Theme.type.silk
                }
            }
            Text {
                topPadding: 12
                width: parent.width
                wrapMode: Text.Wrap
                text: root.lines[root.step].line
                color: Theme.ink1
                font.family: "Yap Bricolage Title"
                font.pixelSize: 30
                font.weight: Font.ExtraBold
                font.letterSpacing: -0.9
                lineHeight: 32
                lineHeightMode: Text.FixedHeight
                Accessible.role: Accessible.StaticText
            }
            Text {
                topPadding: 8
                width: parent.width
                wrapMode: Text.Wrap
                text: root.lines[root.step].detail
                color: Theme.ink2
                font: Theme.type.description
            }
        }
    }
}
