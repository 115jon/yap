import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

// Device-code sign-in: the app shows a code, the user approves it on
// yap.115jon.com/activate in a browser where they are signed in.
Rectangle {
    id: root

    required property var client
    readonly property bool waiting: client.phase === "deviceCode"
    readonly property bool hasCode: client.userCode.length > 0

    color: Theme.ply0

    component KeyButton: Button {
        id: control
        property bool primary: false
        implicitHeight: 48
        leftPadding: 20
        rightPadding: 20
        font.family: Theme.display
        font.pixelSize: 15
        font.weight: Font.DemiBold
        contentItem: Text {
            text: control.text
            font: control.font
            color: control.primary ? Theme.onPrimary : Theme.ink1
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            opacity: control.enabled ? 1 : 0.4
        }
        background: Rectangle {
            radius: Theme.radius
            color: control.primary
                   ? (control.down ? Qt.darker(Theme.primaryFill, 1.15) : control.hovered ? "#ffffff" : Theme.primaryFill)
                   : (control.down ? Theme.ply4 : control.hovered ? Theme.ply3 : "transparent")
            border.width: control.primary ? 0 : 1
            border.color: Theme.ply4
        }
    }

    ColumnLayout {
        anchors.centerIn: parent
        width: Math.min(440, parent.width - 64)
        spacing: 0

        Text {
            text: "YAP · DESKTOP"
            color: Theme.ink3
            font.family: Theme.mono
            font.pixelSize: 11
            font.weight: Font.Medium
            font.letterSpacing: 1
        }
        Text {
            Layout.topMargin: 12
            text: root.waiting ? "Approve this device" : "Sign in"
            color: Theme.ink1
            font.family: Theme.display
            font.pixelSize: 44
            font.weight: Font.ExtraBold
            font.letterSpacing: -1.5
        }
        Text {
            Layout.topMargin: 12
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            text: root.waiting
                  ? "Open the link in a browser where you're signed in to Yap, check the code matches, and approve it."
                  : "You'll get a code to approve in your browser. Nothing to type here."
            color: Theme.ink2
            font.family: Theme.text
            font.pixelSize: 15
            lineHeight: 22
            lineHeightMode: Text.FixedHeight
        }

        // The code, as key caps.
        Row {
            Layout.topMargin: 32
            visible: root.waiting
            spacing: 6
            Repeater {
                model: root.hasCode ? root.client.userCode.split("") : "••••-••••".split("")
                delegate: Rectangle {
                    required property string modelData
                    width: modelData === "-" ? 16 : 44
                    height: 56
                    radius: Theme.radius
                    color: modelData === "-" ? "transparent" : Theme.ply2
                    border.width: modelData === "-" ? 0 : 1
                    border.color: Theme.ply4
                    Text {
                        anchors.centerIn: parent
                        text: parent.modelData
                        color: root.hasCode ? Theme.ink1 : Theme.inkOff
                        font.family: Theme.mono
                        font.pixelSize: 24
                        font.weight: Font.DemiBold
                    }
                }
            }
        }

        Row {
            Layout.topMargin: 16
            visible: root.waiting
            spacing: 8
            Rectangle {
                width: 6; height: 6; radius: 3
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
                text: root.hasCode ? "WAITING FOR APPROVAL" : "GETTING A CODE"
                color: Theme.ink3
                font.family: Theme.mono
                font.pixelSize: 11
                font.letterSpacing: 1
            }
        }

        RowLayout {
            Layout.topMargin: 32
            spacing: 8
            KeyButton {
                visible: !root.waiting
                primary: true
                text: "Get a sign-in code"
                onClicked: root.client.startSignIn()
            }
            KeyButton {
                visible: root.waiting
                primary: true
                enabled: root.hasCode
                text: "Open browser"
                onClicked: Qt.openUrlExternally(root.client.verificationUrl)
            }
            KeyButton {
                visible: root.waiting
                text: "Cancel"
                onClicked: root.client.cancelSignIn()
            }
        }

        Text {
            Layout.topMargin: 16
            Layout.fillWidth: true
            visible: root.client.status.length > 0
            text: root.client.status
            wrapMode: Text.WordWrap
            color: Theme.danger
            font.family: Theme.text
            font.pixelSize: 13
        }

        // Dev: paste a session token instead.
        Text {
            id: devToggle
            Layout.topMargin: 40
            visible: !root.waiting
            text: tokenRow.visible ? "Hide token sign-in" : "Developer: use a session token"
            color: devHover.hovered ? Theme.ink2 : Theme.ink3
            font.family: Theme.text
            font.pixelSize: 12
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
                placeholderTextColor: Theme.inkOff
                color: Theme.ink1
                font.family: Theme.mono
                font.pixelSize: 13
                leftPadding: 12
                background: Rectangle {
                    radius: Theme.radius
                    color: Theme.ply2
                    border.width: 1
                    border.color: tokenField.activeFocus ? Theme.ink3 : Theme.ply4
                }
                onAccepted: useButton.clicked()
            }
            KeyButton {
                id: useButton
                implicitHeight: 40
                text: "Use"
                enabled: tokenField.text.length > 0
                onClicked: {
                    root.client.useToken(tokenField.text)
                    tokenField.clear()
                }
            }
        }
    }
}
