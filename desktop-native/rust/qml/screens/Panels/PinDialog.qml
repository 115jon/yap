import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// PinModal.tsx: "Pin message" / "Unpin message", what it does in the
// channel, the message quoted, the Shift hint (unpin only), then Cancel and
// the confirm. Both are reversible, so pin confirms with the primary fill
// and unpin with the soft destructive tint. The confirm has focus; Esc
// cancels.
Popup {
    id: root

    property string messageId
    property string author
    property string text
    property bool pin: true
    signal confirmed(string id, bool pin)

    function ask(id, author, text, pin) {
        root.messageId = id
        root.author = author
        root.text = text
        root.pin = pin
        open()
    }

    parent: Overlay.overlay
    anchors.centerIn: parent
    width: 440
    modal: true
    // Ctrl+K stays shut while a modal is up (ExtrasHost).
    onVisibleChanged: ShellState.modalCount += visible ? 1 : -1
    focus: true
    padding: 24
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onOpened: confirmKey.forceActiveFocus()

    Overlay.modal: Scrim { color: Theme.scrim }

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
            textFormat: Text.PlainText
            text: root.pin ? qsTr("Pin message") : qsTr("Unpin message")
            color: Theme.ink1
            font: Theme.type.sectionTitle
            Accessible.role: Accessible.Heading
        }
        Text {
            width: parent.width
            text: root.pin
                ? qsTr("Pin this message to #%1. Everyone in the channel can find it in Pinned messages.").arg(Nav.activeChannelName)
                : qsTr("Remove this message from the pins in #%1.").arg(Nav.activeChannelName)
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            color: Theme.ink2
            font: Theme.type.description
        }
        // The message, quoted on the ply below (MessagePreviewCard).
        Rectangle {
            width: parent.width
            height: quote.implicitHeight + 24
            radius: Theme.radius
            color: Theme.ply2
            border.width: 1
            border.color: Theme.line
            Accessible.ignored: true
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
            visible: !root.pin
            width: parent.width
            text: qsTr("Hold Shift when you click Unpin to skip this.")
            wrapMode: Text.Wrap
            color: Theme.ink3
            font: Theme.type.caption
        }
        Row {
            anchors.right: parent.right
            spacing: 8
            PinKey {
                text: qsTr("Cancel")
                onClicked: root.close()
            }
            PinKey {
                id: confirmKey
                text: root.pin ? qsTr("Pin") : qsTr("Unpin")
                tone: root.pin ? "primary" : "destructive"
                onClicked: {
                    root.confirmed(root.messageId, root.pin)
                    root.close()
                }
            }
        }
    }

    component PinKey: Button {
        id: key
        // ghost | primary | destructive
        property string tone: "ghost"
        implicitHeight: 40
        leftPadding: 16
        rightPadding: 16
        font: Theme.type.controlSm
        Keys.onReturnPressed: key.clicked()
        contentItem: Text {
            textFormat: Text.PlainText
            text: key.text
            font: key.font
            color: key.tone === "primary" ? Theme.primaryFillInk
                : key.tone === "destructive" ? Theme.dangerInk : Theme.ink1
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle {
            radius: Theme.radius
            color: key.tone === "primary" ? Theme.primaryFill
                : key.tone === "destructive" ? Theme.dangerTint : "transparent"
            Rectangle {
                anchors.fill: parent
                radius: Theme.radius
                color: key.tone === "primary" ? Theme.primaryFillInk : Theme.ink1
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
