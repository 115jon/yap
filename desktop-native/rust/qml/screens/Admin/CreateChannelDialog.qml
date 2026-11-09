pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import com.yap.native

// Create channel (CreateChannelModal.tsx): the type as two choice cards
// under the fieldset's "Channel type" legend (13/500, 8 above the cards),
// then "Channel name" as a plain label (Label, not a legend plate) over a
// 40 px input with the type's glyph (text names turn lowercase with dashes
// as you type), the name error and "In CATEGORY" or "Not in a category",
// 6 apart; the create error; then Cancel and Create channel as 48 px
// footer keys. An empty name is caught inline; a new text channel opens.
PlateDialog {
    id: dialog

    property string categoryId: ""
    property bool voice: false
    property int doneCount: 0

    readonly property var model: JSON.parse(Admin.state)
    readonly property var act: model.acts["create-channel"] ?? { busy: false, error: "", done: 0 }
    readonly property string nameError: model.acts["create-channel.name"]?.error ?? ""
    readonly property string categoryName: model.categories.find(category => category.id === categoryId)?.name ?? ""

    function openIn(category) {
        categoryId = category
        voice = false
        nameField.text = ""
        Admin.clear("create-channel")
        Admin.clear("create-channel.name")
        doneCount = act.done
        open()
    }
    function create() {
        if (!act.busy)
            Admin.createChannel(nameField.text, voice, categoryId)
    }

    // DialogContent as it stands (the 28/32 title, the close key), top
    // placed as the web's form dialogs are.
    c1: true
    placeTop: true
    title: qsTr("Create channel")
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

    Column {
        width: parent.width
        spacing: 8
        Accessible.role: Accessible.Grouping
        Accessible.name: qsTr("Channel type")
        // <legend className="mb-2 text-ui-sm font-medium">.
        Text {
            text: qsTr("Channel type")
            color: Theme.ink1
            font: Theme.type.occupant
            lineHeight: 18
            lineHeightMode: Text.FixedHeight
        }
        ChoiceCard {
            width: parent.width
            checked: !dialog.voice
            glyph: "textChannel"
            label: qsTr("Text")
            description: qsTr("Send messages, images, GIFs, and more.")
            onClicked: {
                dialog.voice = false
                nameField.text = Admin.sanitizeChannelName(nameField.text, false, false)
            }
        }
        ChoiceCard {
            width: parent.width
            checked: dialog.voice
            glyph: "voiceChannel"
            label: qsTr("Voice")
            description: qsTr("Hang out with voice, video, and screen share.")
            onClicked: dialog.voice = true
        }
    }

    // Label + Input + error + helper (gap-1.5).
    Column {
        width: parent.width
        spacing: 6
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: qsTr("Channel name")
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
            // ps-8: 32 in, the glyph at start-2.5.
            leadingInset: 20
            placeholderText: dialog.voice ? qsTr("General Voice") : qsTr("new-channel")
            invalid: dialog.nameError.length > 0
            Accessible.name: qsTr("Channel name")
            onTextEdited: {
                const clean = Admin.sanitizeChannelName(text, dialog.voice, false)
                if (clean !== text) {
                    const at = cursorPosition
                    text = clean
                    cursorPosition = Math.min(at, clean.length)
                }
                Admin.clear("create-channel.name")
            }
            Keys.onReturnPressed: dialog.create()
            Icon {
                x: 10
                anchors.verticalCenter: parent.verticalCenter
                name: dialog.voice ? "voiceChannel" : "textChannel"
                size: Theme.iconSm
                color: Theme.ink3
            }
        }
        Text {
            visible: dialog.nameError.length > 0
            textFormat: Text.PlainText
            text: dialog.nameError
            color: Theme.dangerInk
            font: Theme.type.caption
            Accessible.role: Accessible.AlertMessage
            Accessible.name: text
        }
        Text {
            textFormat: Text.PlainText
            text: dialog.categoryName.length > 0 ? qsTr("In %1").arg(dialog.categoryName) : qsTr("Not in a category")
            color: Theme.ink3
            font: Theme.type.description
        }
    }
    Text {
        visible: dialog.act.error.length > 0
        width: parent.width
        textFormat: Text.PlainText
        text: dialog.act.error
        wrapMode: Text.Wrap
        color: Theme.dangerInk
        font: Theme.type.caption
        Accessible.role: Accessible.AlertMessage
        Accessible.name: text
    }

    footer: [
        KitButton {
            variant: "ghost"
            size: "lg"
            text: qsTr("Cancel")
            enabled: !dialog.act.busy
            onClicked: dialog.close()
        },
        KitButton {
            variant: "primary"
            size: "lg"
            ground: Theme.ply3
            text: qsTr("Create channel")
            loading: dialog.act.busy
            onClicked: dialog.create()
        }
    ]

    // A text choice card (the settings ChoiceCard recipe): the 16 px radio,
    // the glyph and label, the description; chosen takes the accent ring.
    component ChoiceCard: T.AbstractButton {
        id: card
        property string glyph
        property string label
        property string description
        implicitHeight: cardText.implicitHeight + 24
        focusPolicy: Qt.StrongFocus
        Accessible.role: Accessible.RadioButton
        Accessible.name: label
        Accessible.description: description
        Accessible.checked: checked
        background: Rectangle {
            radius: Theme.radius
            color: card.checked ? Theme.accentTint : card.hovered ? Theme.tint1 : "transparent"
            EaseColor on color {}
            border.width: card.checked ? 2 : 1
            border.color: card.checked ? Theme.accentInk : Theme.line
            Rectangle {
                anchors.fill: parent
                anchors.margins: -4
                visible: card.visualFocus
                color: "transparent"
                radius: Theme.radius
                border.width: 2
                border.color: Theme.focus
            }
        }
        contentItem: Item {
            Rectangle {
                id: radio
                x: 0
                y: 2
                width: 16
                height: 16
                radius: 8
                color: card.checked ? Theme.accentInk : "transparent"
                border.width: 1
                border.color: card.checked ? Theme.accentInk : Theme.lineStrong
                Rectangle {
                    anchors.centerIn: parent
                    visible: card.checked
                    width: 6
                    height: 6
                    radius: 3
                    color: Theme.ply0
                }
            }
            Column {
                id: cardText
                anchors.left: radio.right
                anchors.leftMargin: 12
                anchors.right: parent.right
                spacing: 2
                Row {
                    spacing: 6
                    Icon {
                        anchors.verticalCenter: parent.verticalCenter
                        name: card.glyph
                        size: Theme.iconSm
                        color: card.checked ? Theme.accentInk : Theme.ink3
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: card.label
                        color: Theme.ink1
                        font: Theme.type.row
                    }
                }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: card.description
                    wrapMode: Text.Wrap
                    color: Theme.ink2
                    font: Theme.type.description
                }
            }
        }
        leftPadding: 12
        rightPadding: 12
        topPadding: 12
        bottomPadding: 12
        HoverHandler { cursorShape: Qt.PointingHandCursor }
    }
}
