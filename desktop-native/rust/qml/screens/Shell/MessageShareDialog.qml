pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import Yap.Chrome
import com.yap.native

// Share message (MessageShareModal.tsx): "Creates a public snapshot link",
// the message quoted (with the files the link leaves out), the link
// expiry (7, 30 or 90 days, or never; 30 by default), then Create link.
// Once made, the link is copied and shown read-only with Copy, and the
// footer offers Manage shares, Open and Done.
Popup {
    id: root

    property string messageId
    property string author
    property string text
    property int omitted: 0
    property string expires: "30d"
    property bool copied: false
    readonly property var share: JSON.parse(Extras.share)
    readonly property bool mine: share.messageId === messageId
    readonly property string url: mine ? share.url : ""

    function ask(id, author, text, omitted) {
        Extras.closeShare()
        root.messageId = id
        root.author = author
        root.text = text
        root.omitted = omitted
        root.expires = "30d"
        root.copied = false
        open()
    }

    parent: Overlay.overlay
    anchors.centerIn: parent
    width: 480
    modal: true
    // Ctrl+K stays shut while a modal is up (ExtrasHost).
    onVisibleChanged: ShellState.modalCount += visible ? 1 : -1
    focus: true
    padding: 24
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onOpened: createKey.forceActiveFocus()
    onClosed: Extras.closeShare()

    // The link lands on the clipboard as it is made (the web copies it).
    onUrlChanged: {
        if (url.length > 0) {
            clipboard.copyText(url)
            copied = true
            copiedTimer.restart()
        }
    }

    Timer {
        id: copiedTimer
        interval: 1800
        onTriggered: root.copied = false
    }

    ClipboardText { id: clipboard }

    Overlay.modal: Scrim { color: Theme.scrim }

    background: Rectangle {
        radius: Theme.radiusSheet
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
    }

    enter: PresenceIn { fromScale: Theme.scaleDialog }
    exit: PresenceOut { toScale: Theme.scaleDialogOut }

    contentItem: Column {
        spacing: 16

        Column {
            width: parent.width
            spacing: 4
            Text {
                text: qsTr("Share message")
                color: Theme.ink1
                font: Theme.type.sectionTitle
                Accessible.role: Accessible.Heading
            }
            Text {
                text: qsTr("Creates a public snapshot link")
                color: Theme.ink2
                font: Theme.type.description
            }
        }

        // MessagePreviewCard, inert.
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
                    visible: root.text.length > 0
                    text: root.text
                    wrapMode: Text.Wrap
                    maximumLineCount: 4
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                    color: Theme.ink2
                    font: Theme.type.message
                }
                Text {
                    visible: root.omitted > 0
                    topPadding: 6
                    width: parent.width
                    wrapMode: Text.Wrap
                    text: root.omitted === 1 ? qsTr("1 file isn't included in the public link.")
                        : qsTr("%1 files aren't included in the public link.").arg(root.omitted)
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
        }

        Column {
            visible: root.url.length === 0
            width: parent.width
            spacing: 8
            Text {
                text: qsTr("Link expiry")
                color: Theme.ink1
                font: Theme.type.panelTitle
            }
            Column {
                width: parent.width
                Accessible.role: Accessible.RadioButton
                Repeater {
                    model: [["7d", qsTr("7 days")], ["30d", qsTr("30 days")], ["90d", qsTr("90 days")], ["never", qsTr("Never")]]
                    ExpiryOption {}
                }
            }
            RowError {
                width: parent.width
                above: 4
                text: root.mine ? root.share.error : ""
            }
        }

        Column {
            visible: root.url.length > 0
            width: parent.width
            spacing: 8
            Text {
                text: qsTr("Public link")
                color: Theme.ink1
                font: Theme.type.panelTitle
            }
            Row {
                width: parent.width
                spacing: 8
                KitField {
                    width: parent.width - copyKey.width - 8
                    readOnly: true
                    text: root.url
                    Accessible.name: qsTr("Public link")
                    onActiveFocusChanged: if (activeFocus) selectAll()
                }
                KitButton {
                    id: copyKey
                    variant: "secondary"
                    iconName: root.copied ? "check" : "copy"
                    text: root.copied ? qsTr("Copied") : qsTr("Copy")
                    onClicked: {
                        clipboard.copyText(root.url)
                        root.copied = true
                        copiedTimer.restart()
                    }
                }
            }
        }

        Item {
            width: parent.width
            height: Theme.control
            KitButton {
                visible: root.url.length > 0
                variant: "ghost"
                text: qsTr("Manage shares")
                onClicked: {
                    root.close()
                    SettingsNav.show("privacy", "shared-messages")
                }
            }
            Row {
                anchors.right: parent.right
                spacing: 8
                KitButton {
                    visible: root.url.length === 0
                    variant: "ghost"
                    text: qsTr("Cancel")
                    onClicked: root.close()
                }
                KitButton {
                    id: createKey
                    visible: root.url.length === 0
                    variant: "primary"
                    ground: Theme.ply3
                    iconName: "share"
                    text: qsTr("Create link")
                    loading: root.mine && root.share.busy
                    enabled: !(root.mine && root.share.busy)
                    onClicked: Extras.shareMessage(root.messageId, root.expires)
                }
                KitButton {
                    visible: root.url.length > 0
                    variant: "secondary"
                    iconName: "externalLink"
                    text: qsTr("Open")
                    onClicked: Files.openExternal(root.url)
                }
                KitButton {
                    visible: root.url.length > 0
                    variant: "primary"
                    ground: Theme.ply3
                    text: qsTr("Done")
                    onClicked: root.close()
                }
            }
        }
    }

    component ExpiryOption: AbstractButton {
        id: option
        required property var modelData
        readonly property bool on: root.expires === modelData[0]
        width: parent ? parent.width : 0
        height: 36
        hoverEnabled: true
        enabled: !(root.mine && root.share.busy)
        Accessible.role: Accessible.RadioButton
        Accessible.name: modelData[1]
        Accessible.checked: on
        onClicked: root.expires = modelData[0]
        contentItem: Row {
            spacing: 10
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: 16
                height: 16
                radius: 8
                color: "transparent"
                border.width: option.on ? 5 : 1
                border.color: option.on ? Theme.accentInk : Theme.lineStrong
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: option.modelData[1]
                color: Theme.ink1
                font: Theme.type.row
            }
        }
        background: Rectangle {
            radius: Theme.radius
            color: option.hovered ? Theme.tint1 : "transparent"
            EaseColor on color {}
            border.width: option.visualFocus ? 2 : 0
            border.color: Theme.focus
        }
        HoverHandler { cursorShape: Qt.PointingHandCursor }
    }
}
