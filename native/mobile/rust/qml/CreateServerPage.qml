pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Add a server, a page over the pager from the rail's + key, in two
// numbered sections:
// - Create a server (CreateServerModal.tsx, on the desktop's Yap.Ui
//   ServerCreator): an optional icon from Android's photo picker (the tile
//   or Upload image; Change and Remove once one is picked; the web's copy
//   for a wrong type or one over 8 MB), the name, Create server. The new
//   server opens on its tree (`serverOpened`).
// - Join a server: an invite's link or bare code. A good one opens the
//   invite's page (InviteLandingPage) in place of this one; anything else
//   says so under the field.
// Back waits while a create is on its way.
Rectangle {
    id: root

    readonly property bool sheet: false
    // A create in flight: Back waits for it.
    readonly property bool holdsBack: creator.busy
    property real backProgress: 0

    signal closeRequested()
    // The server just made is the open one (the core selected it).
    signal serverOpened()

    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: Math.max(SafeArea.margins.bottom, Native.gestureInset)
    readonly property real keyboardPad: Math.max(bottomInset, Native.imeHeight)

    // The photo picker is choosing the icon (only this page takes it).
    property bool picking: false
    // Picked copies this page made; each goes once it is not the icon.
    property var picked: []
    property string joinError: ""
    // The field being typed in, kept in view above the keyboard.
    property Item typing: null

    readonly property ServerCreator creator: ServerCreator {
        onCreated: root.serverOpened()
    }
    Component.onCompleted: creator.start()
    Component.onDestruction: root.sweep(!creator.busy)

    function pickIcon() {
        // Another pick still copying (the composer's) would land here.
        if (creator.busy || picking || Native.pickingMedia)
            return
        Qt.inputMethod.hide()
        picking = true
        Native.pickImage(creator.iconLimit)
    }
    Connections {
        target: Native
        function onMediaPicked(paths, failed) {
            if (!root.picking)
                return
            root.picking = false
            root.picked = root.picked.concat(Array.from(paths))
            if (paths.length > 0)
                root.creator.takeIcon(paths[0])
            else if (failed > 0)
                root.creator.iconError = qsTr("The picked image could not be read.")
            root.sweep(false)
        }
        function onImeHeightChanged() {
            if (root.typing)
                Qt.callLater(root.reveal, root.typing)
        }
    }
    // Deletes the picked copies nothing holds (`all`: every one).
    function sweep(all) {
        const left = []
        for (const path of picked) {
            if (!all && path === creator.iconPath)
                left.push(path)
            else
                Native.discardPicked(path)
        }
        picked = left
    }
    Connections {
        target: root.creator
        function onIconPathChanged() { root.sweep(false) }
    }

    function create() {
        Qt.inputMethod.commit()
        creator.create(nameField.text)
        if (nameField.text.trim().length === 0)
            nameField.forceActiveFocus()
    }
    function join() {
        Qt.inputMethod.commit()
        const text = inviteField.text.trim()
        if (text.length === 0) {
            joinError = qsTr("Paste an invite link or code.")
            inviteField.forceActiveFocus()
            return
        }
        joinError = ""
        Qt.inputMethod.hide()
        // A good one opens the invite's page over this one.
        if (!Invites.openInvite(text))
            joinError = qsTr("That isn't an invite link or code.")
    }
    // Scrolls `item` into the part of the page the keyboard leaves.
    function reveal(item) {
        if (!item || !item.visible)
            return
        const at = item.mapToItem(scroll.contentItem, 0, 0)
        const room = scroll.height - root.keyboardPad
        const top = at.y - 16
        const bottom = at.y + item.height + 16
        if (bottom > scroll.contentY + room)
            scroll.contentY = Math.max(0, Math.min(bottom - room, scroll.contentHeight - scroll.height))
        else if (top < scroll.contentY)
            scroll.contentY = Math.max(0, top)
    }

    color: Theme.ply0
    transform: [
        Scale {
            origin.x: root.width / 2
            origin.y: root.height / 2
            xScale: 1 - root.backProgress * 0.08
            yScale: 1 - root.backProgress * 0.08
        },
        Translate { x: root.backProgress * root.width * 0.1 }
    ]

    // Nothing under the page takes a touch.
    MouseArea { anchors.fill: parent }

    PageHeader {
        id: header
        z: 1
        title: qsTr("Add a server")
        topInset: root.topInset
        leftInset: root.leftInset
        rightInset: root.rightInset
        onBackRequested: if (!root.holdsBack) root.closeRequested()
    }

    Flickable {
        id: scroll
        anchors.top: header.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        contentHeight: body.height + root.keyboardPad
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickDeceleration: Native.flingDeceleration
        maximumFlickVelocity: Native.maxFlingVelocity

        Column {
            id: body
            x: Math.max(16 + root.leftInset, (scroll.width - width) / 2)
            width: Math.min(scroll.width - 32 - root.leftInset - root.rightInset, 560)
            topPadding: 20
            bottomPadding: 32
            spacing: 32

            Section {
                number: 1
                title: qsTr("Create a server")
                Note {
                    text: qsTr("Your server is where you and your friends hang out.")
                }
                Row {
                    width: parent.width
                    spacing: 16

                    // The icon key: the picked icon, or the image glyph
                    // (the web's dashed edge as a solid strong line).
                    Rectangle {
                        id: iconKey
                        width: 64
                        height: 64
                        radius: Theme.radius
                        color: iconTap.pressed ? Theme.tint1 : Theme.ply2
                        EaseColor on color {}
                        border.width: 1
                        border.color: Theme.lineStrong
                        clip: true
                        Accessible.role: Accessible.Button
                        Accessible.name: root.creator.iconPath.length > 0 ? qsTr("Change server icon") : qsTr("Upload server icon")
                        Accessible.onPressAction: root.pickIcon()
                        Image {
                            id: preview
                            anchors.fill: parent
                            anchors.margins: 1
                            visible: root.creator.iconUrl.length > 0 && status === Image.Ready
                            source: root.creator.iconUrl
                            sourceSize: Qt.size(128, 128)
                            fillMode: Image.PreserveAspectCrop
                            asynchronous: true
                        }
                        Icon {
                            anchors.centerIn: parent
                            visible: !preview.visible
                            name: "image"
                            size: Theme.iconLg
                            color: Theme.ink3
                        }
                        Spinner {
                            anchors.centerIn: parent
                            visible: root.picking
                            running: visible
                        }
                        TapHandler {
                            id: iconTap
                            onTapped: root.pickIcon()
                        }
                    }
                    Column {
                        width: parent.width - iconKey.width - parent.spacing
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 2
                        Text {
                            width: parent.width
                            textFormat: Text.PlainText
                            text: qsTr("Server icon, optional")
                            elide: Text.ElideRight
                            color: Theme.ink1
                            font: Theme.type.labelStrong
                        }
                        Text {
                            width: parent.width
                            textFormat: Text.PlainText
                            text: root.creator.iconHint
                            wrapMode: Text.Wrap
                            color: Theme.ink3
                            font: Theme.type.caption
                        }
                    }
                }
                Flow {
                    width: parent.width
                    spacing: 8
                    KitButton {
                        variant: "secondary"
                        iconName: "image"
                        text: root.creator.iconPath.length > 0 ? qsTr("Change") : qsTr("Upload image")
                        Accessible.name: root.creator.iconPath.length > 0 ? qsTr("Change server icon") : qsTr("Upload server icon")
                        enabled: !root.creator.busy && !root.picking
                        onClicked: root.pickIcon()
                    }
                    KitButton {
                        visible: root.creator.iconPath.length > 0
                        variant: "ghost"
                        text: qsTr("Remove")
                        Accessible.name: qsTr("Remove server icon")
                        enabled: !root.creator.busy
                        onClicked: root.creator.removeIcon()
                    }
                }
                Column {
                    width: parent.width
                    spacing: 6
                    Text {
                        textFormat: Text.PlainText
                        text: qsTr("Server name")
                        color: Theme.ink1
                        font: Theme.type.labelStrong
                        Accessible.ignored: true
                    }
                    MobileField {
                        id: nameField
                        width: parent.width
                        maximumLength: 100
                        placeholderText: qsTr("My Awesome Server")
                        invalid: root.creator.nameError.length > 0
                        enabled: !root.creator.busy
                        inputMethodHints: Qt.ImhNoPredictiveText
                        Accessible.name: qsTr("Server name")
                        onTextEdited: root.creator.edited()
                        onActiveFocusChanged: root.focused(nameField, activeFocus)
                        onAccepted: root.create()
                    }
                    Error {
                        text: root.creator.nameError
                    }
                }
                Error {
                    text: root.creator.error
                }
                KitButton {
                    width: parent.width
                    size: "lg"
                    variant: "primary"
                    text: qsTr("Create server")
                    loading: root.creator.busy
                    onClicked: root.create()
                }
            }

            Section {
                number: 2
                title: qsTr("Join a server")
                Note {
                    text: qsTr("Have an invite? Paste its link or code.")
                }
                Column {
                    width: parent.width
                    spacing: 6
                    Text {
                        textFormat: Text.PlainText
                        text: qsTr("Invite link or code")
                        color: Theme.ink1
                        font: Theme.type.labelStrong
                        Accessible.ignored: true
                    }
                    MobileField {
                        id: inviteField
                        width: parent.width
                        placeholderText: Admin.webOrigin + "/invite/…"
                        invalid: root.joinError.length > 0
                        enabled: !root.creator.busy
                        inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase | Qt.ImhUrlCharactersOnly
                        Accessible.name: qsTr("Invite link or code")
                        onTextEdited: root.joinError = ""
                        onActiveFocusChanged: root.focused(inviteField, activeFocus)
                        onAccepted: root.join()
                    }
                    Error {
                        text: root.joinError
                    }
                }
                KitButton {
                    width: parent.width
                    size: "lg"
                    variant: "secondary"
                    text: qsTr("Join server")
                    enabled: !root.creator.busy
                    onClicked: root.join()
                }
            }
        }
    }

    function focused(item, on) {
        if (on) {
            typing = item
            Qt.callLater(reveal, item)
        } else if (typing === item) {
            typing = null
        }
    }

    component Section: Column {
        id: section
        property int number: 0
        property string title
        default property alias content: rows.data
        width: parent ? parent.width : 0
        spacing: 0
        Rectangle {
            width: parent.width
            height: 2
            color: Theme.ink1
        }
        Item {
            width: parent.width
            height: 36
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: section.title
                color: Theme.ink2
                font: Theme.type.silk
                Accessible.role: Accessible.Heading
                Accessible.name: section.title
            }
            Text {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                text: String(section.number).padStart(2, "0")
                color: Theme.ink3
                font: Theme.type.numSm
                Accessible.ignored: true
            }
        }
        Column {
            id: rows
            width: parent.width
            spacing: 12
        }
    }

    component Note: Text {
        width: parent ? parent.width : 0
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: Theme.ink2
        font: Theme.type.description
    }

    component Error: Text {
        width: parent ? parent.width : 0
        visible: text.length > 0
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: Theme.dangerInk
        font: Theme.type.caption
        Accessible.role: Accessible.AlertMessage
        Accessible.name: text
    }
}
