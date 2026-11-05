pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import com.yap.native

// Device-code sign-in: the app shows a code, the user approves it on
// yap.115jon.com/activate in a browser where they are signed in.
Rectangle {
    id: root

    readonly property bool waiting: Session.phase === Session.DeviceCode
    readonly property bool hasCode: Session.userCode.length > 0

    color: Theme.ply0

    ColumnLayout {
        anchors.centerIn: parent
        width: Math.min(440, parent.width - 64)
        spacing: 0

        Text {
            text: qsTr("Yap · desktop")
            color: Theme.ink3
            font: Theme.type.silk
        }
        Text {
            Layout.topMargin: 12
            textFormat: Text.PlainText
            text: root.waiting ? qsTr("Approve this device") : qsTr("Sign in")
            color: Theme.ink1
            font: Theme.type.headline
        }
        Text {
            Layout.topMargin: 12
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            textFormat: Text.PlainText
            text: root.waiting
                  ? "Open the link in a browser where you're signed in to Yap, check the code matches, and approve it."
                  : "You'll get a code to approve in your browser. Nothing to type here."
            color: Theme.ink2
            font: Theme.type.message
            lineHeight: Theme.type.messageLine
            lineHeightMode: Text.FixedHeight
        }

        // The code, as key caps.
        Row {
            Layout.topMargin: 32
            visible: root.waiting
            spacing: 6
            Repeater {
                model: root.hasCode ? Session.userCode.split("") : "••••-••••".split("")
                delegate: Rectangle {
                    id: cap
                    required property string modelData
                    width: modelData === "-" ? 16 : 44
                    height: 56
                    radius: Theme.radius
                    color: modelData === "-" ? "transparent" : Theme.ply2
                    border.width: modelData === "-" ? 0 : 1
                    border.color: Theme.lineField
                    Text {
                        anchors.centerIn: parent
                        textFormat: Text.PlainText
                        text: cap.modelData
                        color: root.hasCode ? Theme.ink1 : Theme.inkOff
                        font: Theme.type.deviceCode
                    }
                }
            }
        }

        Row {
            Layout.topMargin: 16
            visible: root.waiting
            spacing: 8
            Led {
                anchors.verticalCenter: parent.verticalCenter
                color: Theme.idle
                SequentialAnimation on opacity {
                    running: root.waiting
                    loops: Animation.Infinite
                    NumberAnimation { to: 0.25; duration: 700 }
                    NumberAnimation { to: 1; duration: 700 }
                }
            }
            Text {
                textFormat: Text.PlainText
                text: root.hasCode ? qsTr("Waiting for approval") : qsTr("Getting a code")
                color: Theme.ink3
                font: Theme.type.silk
            }
        }

        RowLayout {
            Layout.topMargin: 32
            spacing: 8
            KeyButton {
                visible: !root.waiting
                primary: true
                text: "Get a sign-in code"
                onClicked: Session.startSignIn()
            }
            KeyButton {
                visible: root.waiting
                primary: true
                enabled: root.hasCode
                text: "Open browser"
                onClicked: Qt.openUrlExternally(Session.verificationUrl)
            }
            KeyButton {
                visible: root.waiting
                text: "Cancel"
                onClicked: Session.cancelSignIn()
            }
        }

        Text {
            Layout.topMargin: 16
            Layout.fillWidth: true
            visible: Session.status.length > 0
            textFormat: Text.PlainText
            text: Session.status
            wrapMode: Text.WordWrap
            color: Theme.dangerInk
            font: Theme.type.description
        }

        // Dev: paste a session token instead.
        Text {
            Layout.topMargin: 40
            visible: !root.waiting
            textFormat: Text.PlainText
            text: tokenRow.visible ? "Hide token sign-in" : "Developer: use a session token"
            color: devHover.hovered ? Theme.ink2 : Theme.ink3
            font: Theme.type.caption
            HoverHandler { id: devHover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: tokenRow.visible = !tokenRow.visible }
        }
        RowLayout {
            id: tokenRow
            Layout.topMargin: 8
            Layout.fillWidth: true
            visible: false
            spacing: 8
            TextField {
                id: tokenField
                Layout.fillWidth: true
                implicitHeight: 40
                echoMode: TextInput.Password
                placeholderText: "Session token"
                placeholderTextColor: Theme.ink3
                color: Theme.ink1
                selectionColor: Theme.selection
                font: Theme.type.code
                leftPadding: 12
                background: Rectangle {
                    radius: Theme.radius
                    color: Theme.ply2
                    border.width: 1
                    border.color: tokenField.activeFocus ? Theme.ink1 : Theme.lineField
                }
                onAccepted: useButton.clicked()
            }
            KeyButton {
                id: useButton
                implicitHeight: 40
                text: "Use"
                enabled: tokenField.text.length > 0
                onClicked: {
                    Session.useToken(tokenField.text)
                    tokenField.clear()
                }
            }
        }
    }
}
