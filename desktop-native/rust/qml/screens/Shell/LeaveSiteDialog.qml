import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// ExternalLinkConfirmModal.tsx with useExternalLinkConfirmation.tsx: a link
// that leaves the app (another origin, not a trusted host) asks first.
// "Leaving Yap", the full address in a ply-2 well (scrolls past 96 px),
// "Trust <host> links from now on" (kept on this device), Go back and
// Visit site (focused). Only http and https open; the app's own origin
// and trusted hosts open at once.
PlateDialog {
    id: dialog

    property string url
    property string host

    // Opens `value` in the browser, asking first when it leaves the app.
    function request(value) {
        const parsed = /^(https?):\/\/([^\/?#:@]+(?::\d+)?)(?:[\/?#]|$)/i.exec(value.trim())
        if (!parsed)
            return
        const target = value.trim()
        const origin = (parsed[1] + "://" + parsed[2]).toLowerCase()
        const hostname = parsed[2].replace(/:\d+$/, "").toLowerCase()
        if (origin === Admin.webOrigin.toLowerCase() || Desktop.isTrustedLinkHost(hostname)) {
            Qt.openUrlExternally(target)
            return
        }
        dialog.url = target
        dialog.host = hostname
        trust.checked = false
        open()
    }

    width: Math.min(384, parent.width - 32)
    title: qsTr("Leaving Yap")
    description: qsTr("This link opens the website below in your browser.")
    onOpened: visitKey.forceActiveFocus()

    Rectangle {
        width: parent.width
        height: Math.min(96, address.implicitHeight + 16)
        radius: Theme.radius
        color: Theme.ply2
        border.width: 1
        border.color: Theme.line
        clip: true
        Flickable {
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            anchors.topMargin: 8
            anchors.bottomMargin: 8
            contentHeight: address.implicitHeight
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ThinScrollBar {}
            TextEdit {
                id: address
                width: parent.width
                readOnly: true
                selectByMouse: true
                textFormat: TextEdit.PlainText
                wrapMode: TextEdit.WrapAnywhere
                text: dialog.url
                color: Theme.ink1
                selectionColor: Theme.selection
                font: Theme.type.description
                Accessible.name: qsTr("Link address")
            }
        }
    }

    CheckBox {
        id: trust
        HoverHandler { cursorShape: Qt.PointingHandCursor }
        width: parent.width
        text: qsTr("Trust %1 links from now on").arg(dialog.host)
        padding: 0
        spacing: 8
        font: Theme.type.description
        indicator: Rectangle {
            x: trust.leftPadding
            anchors.verticalCenter: parent.verticalCenter
            implicitWidth: 16
            implicitHeight: 16
            radius: Theme.radius
            color: trust.checked ? Theme.primaryFill : "transparent"
            border.width: trust.checked ? 0 : 1
            border.color: trust.visualFocus ? Theme.focus : Theme.lineStrong
            Icon {
                anchors.centerIn: parent
                visible: trust.checked
                name: "check"
                size: Theme.iconXs
                color: Theme.primaryFillInk
            }
        }
        contentItem: Text {
            leftPadding: trust.indicator.width + trust.spacing
            verticalAlignment: Text.AlignVCenter
            text: trust.text
            font: trust.font
            wrapMode: Text.WrapAnywhere
            color: trust.hovered ? Theme.ink1 : Theme.ink2
            EaseColor on color {}
        }
    }

    footer: [
        KitButton {
            variant: "ghost"
            text: qsTr("Go back")
            onClicked: dialog.close()
        },
        KitButton {
            id: visitKey
            variant: "primary"
            ground: Theme.ply3
            text: qsTr("Visit site")
            onClicked: {
                if (trust.checked)
                    Desktop.trustLinkHost(dialog.host)
                dialog.close()
                Qt.openUrlExternally(dialog.url)
            }
        }
    ]
}
