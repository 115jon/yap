import QtQuick
import Yap.Chrome
import com.yap.native
import Yap.Ui

// Create a server (CreateServerModal.tsx): the icon key (64 px, a dashed
// edge; Upload image or Change, and Remove), "Server icon, optional" with
// the formats, the icon or create error, then Server name as a plain label
// over a 40 px input (Label + Input, gap 6: no legend here, unlike the
// invite dialog); Cancel and Create server as 48 px footer keys. An empty
// name is caught inline. The icon goes up first, then the server; it opens
// once created. No templates and no join path: the web's modal has neither.
PlateDialog {
    id: dialog

    // The form's state, shared with the phone (Yap.Ui ServerCreator).
    readonly property ServerCreator creator: ServerCreator {
        onCreated: if (dialog.opened) dialog.close()
    }
    readonly property string iconPath: creator.iconPath

    function start() {
        nameField.text = ""
        creator.start()
        open()
    }
    function create() {
        if (creator.busy)
            return
        creator.create(nameField.text)
        if (nameField.text.trim().length === 0)
            nameField.forceActiveFocus()
    }
    function pickIcon() {
        const paths = Files.pickFiles(qsTr("Upload server icon"))
        creator.takeIcon(paths.length > 0 ? paths[0] : "")
    }

    // DialogContent as it stands (the 28/32 title, the close key), top
    // placed as the web's form dialogs are.
    c1: true
    placeTop: true
    title: qsTr("Create a server")
    description: qsTr("Your server is where you and your friends hang out.")
    busy: creator.busy
    onOpened: nameField.forceActiveFocus()

    Row {
        width: parent.width
        spacing: 16

        Rectangle {
            id: iconKey
            width: 64
            height: 64
            radius: Theme.radius
            color: iconHover.hovered ? Theme.tint1 : Theme.ply2
            EaseColor on color {}
            activeFocusOnTab: true
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Server icon, optional")
            Accessible.description: dialog.creator.iconHint
            Accessible.onPressAction: dialog.pickIcon()
            Keys.onSpacePressed: dialog.pickIcon()
            Keys.onReturnPressed: dialog.pickIcon()

            // border border-dashed border-line-strong.
            DashedEdge { color: Theme.lineStrong }
            Image {
                id: preview
                anchors.fill: parent
                anchors.margins: 1
                visible: dialog.creator.iconUrl.length > 0 && status === Image.Ready
                source: dialog.creator.iconUrl
                sourceSize: Qt.size(128, 128)
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
            }
            Rectangle {
                anchors.fill: parent
                anchors.margins: 1
                visible: preview.visible
                // A media overlay: bg-black/48 whatever the theme.
                color: Qt.rgba(0, 0, 0, 0.48)
                opacity: iconHover.hovered || iconKey.activeFocus ? 1 : 0
                Behavior on opacity { NumberAnimation { duration: Theme.dur1 } }
            }
            Icon {
                anchors.centerIn: parent
                visible: !preview.visible || iconHover.hovered || iconKey.activeFocus
                name: "image"
                size: Theme.iconLg
                color: preview.visible ? "white" : iconHover.hovered ? Theme.ink1 : Theme.ink3
            }
            Rectangle {
                anchors.fill: parent
                anchors.margins: -3
                visible: iconKey.activeFocus
                radius: Theme.radius + 2
                color: "transparent"
                border.width: 2
                border.color: Theme.focus
            }
            HoverHandler { id: iconHover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: dialog.pickIcon() }
        }

        Column {
            width: parent.width - iconKey.width - parent.spacing
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            // text-ui-sm font-medium.
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: qsTr("Server icon, optional")
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.occupant
                lineHeight: 18
                lineHeightMode: Text.FixedHeight
            }
            Text {
                width: parent.width
                text: dialog.creator.iconHint
                wrapMode: Text.Wrap
                color: Theme.ink3
                font: Theme.type.caption
            }
            Row {
                topPadding: 6
                spacing: 4
                KitButton {
                    variant: "secondary"
                    size: "sm"
                    text: dialog.iconPath.length > 0 ? qsTr("Change") : qsTr("Upload image")
                    onClicked: dialog.pickIcon()
                }
                KitButton {
                    visible: dialog.iconPath.length > 0
                    variant: "ghost"
                    size: "sm"
                    text: qsTr("Remove")
                    onClicked: dialog.creator.removeIcon()
                }
            }
        }
    }
    Text {
        visible: dialog.creator.error.length > 0
        width: parent.width
        textFormat: Text.PlainText
        text: dialog.creator.error
        wrapMode: Text.Wrap
        color: Theme.dangerInk
        font: Theme.type.caption
        Accessible.role: Accessible.AlertMessage
        Accessible.name: text
    }

    // Label + Input (gap-1.5): a plain 13/500 label over the 40 px field.
    Column {
        width: parent.width
        spacing: 6
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: qsTr("Server name")
            elide: Text.ElideRight
            color: Theme.ink1
            font: Theme.type.occupant
            lineHeight: 18
            lineHeightMode: Text.FixedHeight
            Accessible.ignored: true
        }
        KitField {
            id: nameField
            width: parent.width
            maximumLength: 100
            placeholderText: qsTr("My Awesome Server")
            invalid: dialog.creator.nameError.length > 0
            Accessible.name: qsTr("Server name")
            onTextEdited: dialog.creator.edited()
            Keys.onReturnPressed: dialog.create()
            Keys.onEnterPressed: dialog.create()
        }
        Text {
            visible: dialog.creator.nameError.length > 0
            width: parent.width
            textFormat: Text.PlainText
            text: dialog.creator.nameError
            color: Theme.dangerInk
            font: Theme.type.caption
            Accessible.role: Accessible.AlertMessage
            Accessible.name: text
        }
    }

    footer: [
        KitButton {
            variant: "ghost"
            size: "lg"
            text: qsTr("Cancel")
            enabled: !dialog.creator.busy
            onClicked: dialog.close()
        },
        KitButton {
            variant: "primary"
            size: "lg"
            ground: Theme.ply3
            text: qsTr("Create server")
            loading: dialog.creator.busy
            onClicked: dialog.create()
        }
    ]
}
