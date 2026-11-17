import QtQuick
import QtQuick.Layouts
import Yap.Ui

// Joining a meeting room (the web's RoomJoinForm, RoomPageClient.tsx): the
// room's link or code, the name the room will see, and Join. Both apps
// place it their own way (the desktop's sign-in page and dialog, later
// the phone's page); the rules for a code and a name are the core's
// (yap_core::rooms, through Voice.roomSlug and Voice.roomName).
ColumnLayout {
    id: form

    // A new meeting: the code is made for it and only the name is asked.
    property bool starting: false
    // The ply the form sits on (the Join key's notch shows it).
    property color ground: Theme.ply0

    // The join went to the call, as `name`.
    signal joined(string slug, string name)

    property string error: ""

    // Fills the form: the room a link named (or a new one's code) and the
    // name to start from (the account's, when signed in).
    function reset(room, name) {
        linkField.text = room
        nameField.text = name
        form.error = ""
    }

    function focusFirst() {
        if (!form.starting && linkField.text.length === 0)
            linkField.forceActiveFocus()
        else
            nameField.forceActiveFocus()
    }

    function submit() {
        const slug = Voice.roomSlug(linkField.text)
        const name = Voice.roomName(nameField.text)
        if (slug.length === 0) {
            form.error = qsTr("Paste a meeting link or code.")
            linkField.forceActiveFocus()
            return
        }
        if (name.length === 0) {
            form.error = qsTr("Enter a name of up to 32 characters.")
            nameField.forceActiveFocus()
            return
        }
        form.error = ""
        Voice.joinRoom(slug, name)
        form.joined(slug, name)
    }

    spacing: 0

    Text {
        visible: !form.starting
        Layout.fillWidth: true
        textFormat: Text.PlainText
        text: qsTr("Meeting link or code")
        color: Theme.ink2
        font: Theme.type.labelStrong
    }
    KitField {
        id: linkField
        visible: !form.starting
        Layout.topMargin: 8
        Layout.fillWidth: true
        large: true
        placeholderText: qsTr("standup-room")
        invalid: form.error.length > 0 && Voice.roomSlug(text).length === 0
        Accessible.name: qsTr("Meeting link or code")
        onTextEdited: form.error = ""
        onAccepted: nameField.forceActiveFocus()
    }

    Text {
        Layout.topMargin: form.starting ? 0 : 20
        Layout.fillWidth: true
        textFormat: Text.PlainText
        text: qsTr("Your name")
        color: Theme.ink2
        font: Theme.type.labelStrong
    }
    KitField {
        id: nameField
        Layout.topMargin: 8
        Layout.fillWidth: true
        large: true
        maximumLength: 64
        placeholderText: qsTr("Ada Lovelace")
        invalid: form.error.length > 0 && Voice.roomName(text).length === 0
        Accessible.name: qsTr("Your name")
        onTextEdited: form.error = ""
        onAccepted: form.submit()
    }
    Text {
        Layout.topMargin: 8
        Layout.fillWidth: true
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        text: form.error.length > 0 ? form.error : qsTr("Everyone in the meeting sees this name.")
        color: form.error.length > 0 ? Theme.dangerInk : Theme.ink3
        font: Theme.type.description
        Accessible.role: form.error.length > 0 ? Accessible.AlertMessage : Accessible.StaticText
    }

    KitButton {
        Layout.topMargin: 24
        Layout.fillWidth: true
        variant: "primary"
        size: "lg"
        ground: form.ground
        text: form.starting ? qsTr("Start meeting") : qsTr("Join meeting")
        onClicked: form.submit()
    }
}
