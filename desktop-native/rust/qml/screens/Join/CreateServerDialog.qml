import QtQuick
import com.yap.native

// Create a server (CreateServerModal.tsx): the icon key (64 px, Upload
// image or Change, and Remove), "Server icon, optional" with the formats,
// the icon or create error, then Server name; Cancel and Create server. An
// empty name is caught inline. The icon goes up first, then the server;
// it opens once created. No templates and no join path: the web's modal
// has neither.
PlateDialog {
    id: dialog

    property string iconPath: ""
    property string iconError: ""
    property int doneCount: 0
    // Typing clears the name error (SET_NAME), until the next Create.
    property bool nameEdited: false
    readonly property var model: JSON.parse(Invites.state)
    readonly property var act: model.create ?? { busy: false, error: "", done: 0 }
    readonly property string nameError: nameEdited ? "" : model.createNameError ?? ""
    readonly property string shownError: iconError.length > 0 ? iconError : act.error
    readonly property string iconUrl: iconPath.length > 0 ? "file:///" + iconPath.replace(/\\/g, "/") : ""

    function start() {
        nameField.text = ""
        iconPath = ""
        iconError = ""
        nameEdited = false
        Invites.startCreate()
        doneCount = act.done
        open()
    }
    function create() {
        if (act.busy)
            return
        iconError = ""
        nameEdited = false
        Invites.createServer(nameField.text, iconPath)
        if (nameField.text.trim().length === 0)
            nameField.forceActiveFocus()
    }
    function pickIcon() {
        const paths = Files.pickFiles(qsTr("Upload server icon"))
        const path = paths.length > 0 ? paths[0] : ""
        if (path.length === 0)
            return
        const problem = Invites.iconProblem(path)
        iconError = problem
        if (problem.length === 0)
            iconPath = path
    }

    title: qsTr("Create a server")
    description: qsTr("Your server is where you and your friends hang out.")
    busy: act.busy
    onOpened: nameField.forceActiveFocus()
    onActChanged: {
        if (act.done < doneCount)
            doneCount = act.done
        else if (opened && act.done > doneCount) {
            doneCount = act.done
            close()
        }
    }

    Row {
        width: parent.width
        spacing: 16

        Rectangle {
            id: iconKey
            width: 64
            height: 64
            radius: Theme.radius
            color: iconHover.hovered ? Theme.tint1 : Theme.ply2
            border.width: 1
            border.color: Theme.lineStrong
            activeFocusOnTab: true
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Server icon, optional")
            Accessible.description: qsTr("PNG, JPG, GIF or WebP, up to 8 MB.")
            Accessible.onPressAction: dialog.pickIcon()
            Keys.onSpacePressed: dialog.pickIcon()
            Keys.onReturnPressed: dialog.pickIcon()

            Image {
                id: preview
                anchors.fill: parent
                anchors.margins: 1
                visible: dialog.iconUrl.length > 0 && status === Image.Ready
                source: dialog.iconUrl
                sourceSize: Qt.size(128, 128)
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
            }
            Rectangle {
                anchors.fill: parent
                anchors.margins: 1
                visible: preview.visible
                color: Theme.scrim
                opacity: iconHover.hovered || iconKey.activeFocus ? 1 : 0
                Behavior on opacity { OpacityAnimator { duration: Theme.dur1 } }
            }
            Icon {
                anchors.centerIn: parent
                visible: !preview.visible || iconHover.hovered || iconKey.activeFocus
                name: "image"
                size: Theme.iconLg
                color: preview.visible ? Theme.ink1 : iconHover.hovered ? Theme.ink1 : Theme.ink3
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
            Text {
                width: parent.width
                text: qsTr("Server icon, optional")
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.setting
            }
            Text {
                width: parent.width
                text: qsTr("PNG, JPG, GIF or WebP, up to 8 MB.")
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
                    onClicked: {
                        dialog.iconPath = ""
                        dialog.iconError = ""
                    }
                }
            }
        }
    }
    Text {
        visible: dialog.shownError.length > 0
        width: parent.width
        text: dialog.shownError
        wrapMode: Text.Wrap
        color: Theme.dangerInk
        font: Theme.type.caption
        Accessible.role: Accessible.AlertMessage
        Accessible.name: text
    }

    Column {
        width: parent.width
        spacing: 6
        FieldPlate {
            width: parent.width
            label: qsTr("Server name")
            ground: Theme.ply3
            KitField {
                id: nameField
                large: true
                maximumLength: 100
                placeholderText: qsTr("My Awesome Server")
                invalid: dialog.nameError.length > 0
                Accessible.name: qsTr("Server name")
                onTextEdited: dialog.nameEdited = true
                Keys.onReturnPressed: dialog.create()
                Keys.onEnterPressed: dialog.create()
            }
        }
        Text {
            visible: dialog.nameError.length > 0
            width: parent.width
            text: dialog.nameError
            color: Theme.dangerInk
            font: Theme.type.caption
            Accessible.role: Accessible.AlertMessage
            Accessible.name: text
        }
    }

    footer: [
        KitButton {
            variant: "ghost"
            text: qsTr("Cancel")
            enabled: !dialog.act.busy
            onClicked: dialog.close()
        },
        KitButton {
            variant: "primary"
            ground: Theme.ply3
            text: qsTr("Create server")
            loading: dialog.act.busy
            onClicked: dialog.create()
        }
    ]
}
