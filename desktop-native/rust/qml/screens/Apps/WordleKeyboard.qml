pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import com.yap.native

// The on-screen keyboard: three rows, Enter and Backspace on the last,
// each letter coloured by its best mark so far.
Column {
    id: root

    property var keys: ({})
    spacing: 6

    Repeater {
        model: ["qwertyuiop", "asdfghjkl", "+zxcvbnm-"]
        delegate: Row {
            id: keyRow
            required property string modelData
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 4
            Repeater {
                model: keyRow.modelData.split("")
                delegate: T.Button {
                    id: key
                    required property string modelData
                    readonly property bool wide: modelData === "+" || modelData === "-"
                    readonly property string mark: root.keys[modelData] ?? ""
                    width: wide ? 64 : 40
                    height: 48
                    focusPolicy: Qt.NoFocus
                    Accessible.role: Accessible.Button
                    Accessible.name: modelData === "+" ? qsTr("Enter") : modelData === "-" ? qsTr("Backspace") : modelData
                    onClicked: Apps.wordleKey(modelData === "+" ? "enter" : modelData === "-" ? "backspace" : modelData)
                    background: Rectangle {
                        radius: 4
                        color: key.mark === "correct" ? Theme.talk : key.mark === "present" ? Theme.idle
                            : key.mark === "absent" ? Theme.ply1 : Theme.tint2
                        Rectangle {
                            anchors.fill: parent
                            radius: parent.radius
                            color: Theme.ink1
                            opacity: key.down ? 0.14 : key.hovered ? 0.08 : 0
                        }
                    }
                    contentItem: Item {
                        Text {
                            visible: !key.wide
                            anchors.centerIn: parent
                            textFormat: Text.PlainText
                            text: key.modelData.toUpperCase()
                            color: key.mark === "correct" ? Theme.talkFillInk : key.mark === "absent" ? Theme.ink3 : Theme.ink1
                            font: Theme.type.row
                        }
                        Icon {
                            visible: key.wide
                            anchors.centerIn: parent
                            name: key.modelData === "+" ? "check" : "backspace"
                            size: Theme.iconMd
                            color: Theme.ink1
                        }
                    }
                }
            }
        }
    }
}
