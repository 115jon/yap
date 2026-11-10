import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// The composer, phone layout: a growing field (up to five lines) and a send
// key. The IME shows its Send action (EnterKey); autocorrect and
// suggestions stay on, as in any Android messaging app. The desktop's
// Composer adds uploads, GIFs, emoji and mentions through desktop pickers.
Rectangle {
    id: root

    property string placeholder: ""

    signal send(string text)

    function submit() {
        const text = field.text.trim()
        if (text.length === 0)
            return
        root.send(text)
        field.clear()
    }

    implicitHeight: Math.min(field.implicitHeight, 5 * Theme.type.messageLine + 20) + 16
    color: Theme.ply0

    Rectangle {
        anchors.top: parent.top
        width: parent.width
        height: 1
        color: Theme.line
    }

    Rectangle {
        id: plate
        anchors.left: parent.left
        anchors.leftMargin: 12
        anchors.right: sendKey.left
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        height: parent.height - 16
        radius: height > 48 ? Theme.radiusSheet * 3 : height / 2
        color: Theme.ply1
        border.color: field.activeFocus ? Theme.lineStrong : Theme.line

        Flickable {
            id: flick
            anchors.fill: parent
            anchors.leftMargin: 14
            anchors.rightMargin: 14
            contentHeight: field.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            flickableDirection: Flickable.VerticalFlick

            TextArea.flickable: TextArea {
                id: field
                wrapMode: TextEdit.Wrap
                color: Theme.ink1
                placeholderText: root.placeholder
                placeholderTextColor: Theme.ink3
                font: Theme.type.message
                topPadding: 10
                bottomPadding: 10
                leftPadding: 0
                rightPadding: 0
                background: null
                selectByMouse: true
                // Text selection on touch: Qt draws the platform's handles
                // and its cut/copy/paste bar (QtQuick native handles).
                persistentSelection: true
                EnterKey.type: Qt.EnterKeySend
                Keys.onReturnPressed: event => {
                    // Shift+Enter (a hardware keyboard) is a new line.
                    if (event.modifiers & Qt.ShiftModifier) {
                        event.accepted = false
                        return
                    }
                    root.submit()
                }
                onTextChanged: if (text.length > 0) Chat.sendTyping()
            }
        }
    }

    IconKey {
        id: sendKey
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        iconName: "send"
        tip: qsTr("Send")
        enabled: field.text.trim().length > 0
        onClicked: root.submit()
    }
}
