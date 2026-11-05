import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// DeleteMessageDialog.tsx: title, consequence, the message itself, the
// Shift hint, then Cancel and Delete. Delete has focus; Esc cancels.
Popup {
    id: root

    property string messageId
    property string author
    property string text
    signal confirmed(string id)

    function ask(id, author, text) {
        root.messageId = id
        root.author = author
        root.text = text
        open()
    }

    parent: Overlay.overlay
    anchors.centerIn: parent
    width: 440
    modal: true
    focus: true
    padding: 24
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onOpened: deleteKey.forceActiveFocus()

    Overlay.modal: Rectangle { color: Theme.scrim }

    background: Rectangle {
        radius: Theme.radiusSheet
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
    }

    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
            NumberAnimation { property: "scale"; from: Theme.scaleDialog; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
        }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: Theme.dur3Exit }
    }

    contentItem: Column {
        spacing: 16

        Text {
            width: parent.width
            text: qsTr("Delete message")
            color: Theme.ink1
            font: Theme.type.sectionTitle
            Accessible.role: Accessible.Heading
        }
        Text {
            width: parent.width
            text: qsTr("This deletes the message for everyone. You can't undo it.")
            wrapMode: Text.Wrap
            color: Theme.ink2
            font: Theme.type.description
        }
        // The message, quoted on the ply below.
        Rectangle {
            width: parent.width
            height: quote.implicitHeight + 24
            radius: Theme.radius
            color: Theme.ply2
            border.width: 1
            border.color: Theme.line
            Column {
                id: quote
                x: 12
                y: 12
                width: parent.width - 24
                spacing: 2
                Text {
                    textFormat: Text.PlainText
                    text: root.author
                    color: Theme.ink1
                    font: Theme.type.author
                }
                Text {
                    width: parent.width
                    text: root.text
                    wrapMode: Text.Wrap
                    maximumLineCount: 4
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                    color: Theme.ink2
                    font: Theme.type.message
                }
            }
        }
        Text {
            width: parent.width
            text: qsTr("Hold Shift when you click Delete to skip this.")
            wrapMode: Text.Wrap
            color: Theme.ink3
            font: Theme.type.caption
        }
        Row {
            anchors.right: parent.right
            spacing: 8
            DialogKey {
                text: qsTr("Cancel")
                onClicked: root.close()
            }
            DialogKey {
                id: deleteKey
                text: qsTr("Delete")
                danger: true
                onClicked: {
                    root.confirmed(root.messageId)
                    root.close()
                }
            }
        }
    }

    component DialogKey: Button {
        id: key
        property bool danger: false
        implicitHeight: 40
        leftPadding: 16
        rightPadding: 16
        font: Theme.type.controlSm
        Keys.onReturnPressed: key.clicked()
        contentItem: Text {
            textFormat: Text.PlainText
            text: key.text
            font: key.font
            color: key.danger ? Theme.dangerFillInk : Theme.ink1
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle {
            radius: Theme.radius
            color: key.danger ? Theme.dangerFill : "transparent"
            Rectangle {
                anchors.fill: parent
                radius: Theme.radius
                color: key.danger ? Theme.dangerFillInk : Theme.ink1
                opacity: key.down ? 0.14 : key.hovered ? 0.08 : 0
            }
            Rectangle {
                anchors.fill: parent
                anchors.margins: -3
                visible: key.visualFocus
                color: "transparent"
                radius: Theme.radius
                border.width: 2
                border.color: Theme.focus
            }
        }
    }
}
