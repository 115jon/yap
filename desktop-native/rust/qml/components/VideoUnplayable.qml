import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// A video that will not play (VideoAttachment.tsx `mediaError`): ply-2,
// p-5, the 32 px alert in ink-3, "This video cannot be played here" at
// 13/600 over the codec line at 12 ink-2 (max-w-80), 12 apart, then the
// 32 px tint-1 Download video key (16 px glyph, 13/500). At least 160 high.
Rectangle {
    id: root

    property bool showDownload: true
    signal downloadRequested()

    implicitWidth: Math.min(360, column.implicitWidth + 40)
    implicitHeight: Math.max(160, column.implicitHeight + 40)
    color: Theme.ply2

    Column {
        id: column
        anchors.centerIn: parent
        width: Math.min(root.width - 40, 320)
        spacing: 12
        Icon {
            anchors.horizontalCenter: parent.horizontalCenter
            name: "alert"
            size: 32
            color: Theme.ink3
        }
        Column {
            width: parent.width
            spacing: 4
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                textFormat: Text.PlainText
                text: qsTr("This video cannot be played here")
                wrapMode: Text.Wrap
                color: Theme.ink1
                font: Theme.type.labelStrong
            }
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                textFormat: Text.PlainText
                text: qsTr("The desktop video engine may not support this file's codec.")
                wrapMode: Text.Wrap
                color: Theme.ink2
                font: Theme.type.caption
            }
        }
        AbstractButton {
            id: key
            visible: root.showDownload
            anchors.horizontalCenter: parent.horizontalCenter
            height: 32
            width: keyRow.implicitWidth + 24
            hoverEnabled: true
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Download video")
            HoverHandler { cursorShape: Qt.PointingHandCursor }
            onClicked: root.downloadRequested()
            background: Rectangle {
                radius: Theme.radius
                color: key.hovered ? Theme.tint2 : Theme.tint1
                Behavior on color { ColorAnimation { duration: Theme.dur1 } }
            }
            contentItem: Item {
                Row {
                    id: keyRow
                    anchors.centerIn: parent
                    spacing: 8
                    Icon {
                        anchors.verticalCenter: parent.verticalCenter
                        name: "download"
                        size: Theme.iconSm
                        color: Theme.ink1
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: qsTr("Download video")
                        color: Theme.ink1
                        font: Qt.font({ family: "Yap Geist Medium", pixelSize: 13, weight: Font.Medium })
                    }
                }
            }
        }
    }
}
