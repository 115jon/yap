pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// Create channel (CreateChannelModal.tsx): the type as two choice cards
// under the fieldset's "Channel type" legend (13/500, 8 above the cards),
// then "Channel name" as a plain label (Label, not a legend plate) over a
// 40 px input with the type's glyph (text names turn lowercase with dashes
// as you type), the name error and "In CATEGORY" or "Not in a category",
// 6 apart; the create error; then Cancel and Create channel as 48 px
// footer keys. An empty name is caught inline; a new text channel opens.
PlateDialog {
    id: dialog

    // The form's state, shared with the phone (Yap.Ui ChannelCreator).
    readonly property ChannelCreator creator: ChannelCreator {
        onCreated: if (dialog.opened) dialog.close()
    }

    function openIn(category) {
        nameField.text = ""
        creator.start(category)
        open()
    }
    function create() {
        creator.create(nameField.text)
    }

    // DialogContent as it stands (the 28/32 title, the close key), top
    // placed as the web's form dialogs are.
    c1: true
    placeTop: true
    title: qsTr("Create channel")
    busy: creator.busy
    onOpened: nameField.forceActiveFocus()

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
            checked: !dialog.creator.voice
            glyph: "textChannel"
            label: qsTr("Text")
            description: dialog.creator.textHint
            onClicked: {
                dialog.creator.voice = false
                nameField.text = dialog.creator.clean(nameField.text)
            }
        }
        ChoiceCard {
            width: parent.width
            checked: dialog.creator.voice
            glyph: "voiceChannel"
            label: qsTr("Voice")
            description: dialog.creator.voiceHint
            onClicked: dialog.creator.voice = true
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
            placeholderText: dialog.creator.placeholder
            invalid: dialog.creator.nameError.length > 0
            Accessible.name: qsTr("Channel name")
            onTextEdited: {
                const clean = dialog.creator.clean(text)
                if (clean !== text) {
                    const at = cursorPosition
                    text = clean
                    cursorPosition = Math.min(at, clean.length)
                }
                dialog.creator.edited()
            }
            Keys.onReturnPressed: dialog.create()
            Icon {
                x: 10
                anchors.verticalCenter: parent.verticalCenter
                name: dialog.creator.voice ? "voiceChannel" : "textChannel"
                size: Theme.iconSm
                color: Theme.ink3
            }
        }
        Text {
            visible: dialog.creator.nameError.length > 0
            textFormat: Text.PlainText
            text: dialog.creator.nameError
            color: Theme.dangerInk
            font: Theme.type.caption
            Accessible.role: Accessible.AlertMessage
            Accessible.name: text
        }
        Text {
            textFormat: Text.PlainText
            text: dialog.creator.placeLine
            color: Theme.ink3
            font: Theme.type.description
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
            text: qsTr("Create channel")
            loading: dialog.creator.busy
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
