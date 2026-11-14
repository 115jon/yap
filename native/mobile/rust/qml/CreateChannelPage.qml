pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Create channel, a page over the pager from the server's menu
// (CreateChannelModal.tsx, on the desktop's Yap.Ui ChannelCreator): the
// type as two choice cards (Text, Voice), the channel's name with the
// type's glyph (a text name turns lowercase with dashes as it is typed),
// "In CATEGORY" or "Not in a category" under it, then Create channel. The
// server's menu offers it only with Manage channels. A new text channel
// opens in the chat (`conversationRequested`); a new voice one closes the
// page on the tree. Back waits while the create is on its way.
Rectangle {
    id: root

    // The category the channel goes in (empty: none).
    property string categoryId: ""
    readonly property bool sheet: false
    readonly property bool holdsBack: creator.busy
    property real backProgress: 0

    signal closeRequested()
    signal conversationRequested(string channelId)

    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: Math.max(SafeArea.margins.bottom, Native.gestureInset)
    readonly property real keyboardPad: Math.max(bottomInset, Native.imeHeight)

    // The open channel when Create was pressed: the core opens a new text
    // channel, so another open one (now or a moment later; Nav and Admin
    // need not update in order) is the new one.
    property string channelBefore: ""
    property bool awaitingChannel: false
    readonly property ChannelCreator creator: ChannelCreator {
        onCreated: {
            if (root.creator.voice) {
                root.closeRequested()
                return
            }
            root.awaitingChannel = true
            root.takeChannel()
            if (root.awaitingChannel)
                awaitTimer.restart()
        }
    }
    function takeChannel() {
        if (!awaitingChannel || Nav.activeChannelId.length === 0 || Nav.activeChannelId === channelBefore)
            return
        awaitingChannel = false
        conversationRequested(Nav.activeChannelId)
    }
    Connections {
        target: Nav
        function onActiveChannelIdChanged() { root.takeChannel() }
    }
    // It never opened (the server was left meanwhile): back to the tree.
    Timer {
        id: awaitTimer
        interval: 3000
        onTriggered: {
            if (root.awaitingChannel) {
                root.awaitingChannel = false
                root.closeRequested()
            }
        }
    }
    // No focus on the name at once: focus given while the page is still
    // being pushed raises the keyboard, but its typing never reaches the
    // field.
    Component.onCompleted: creator.start(categoryId)

    function create() {
        Qt.inputMethod.commit()
        channelBefore = Nav.activeChannelId
        creator.create(nameField.text)
        if (creator.nameError.length > 0)
            nameField.forceActiveFocus()
    }
    function pick(voice) {
        if (creator.busy)
            return
        creator.voice = voice
        nameField.text = creator.clean(nameField.text)
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
        title: qsTr("Create channel")
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
            spacing: 24

            Column {
                width: parent.width
                spacing: 8
                Accessible.role: Accessible.Grouping
                Accessible.name: qsTr("Channel type")
                Text {
                    textFormat: Text.PlainText
                    text: qsTr("Channel type")
                    color: Theme.ink1
                    font: Theme.type.labelStrong
                    Accessible.ignored: true
                }
                ChoiceCard {
                    width: parent.width
                    checked: !root.creator.voice
                    glyph: "textChannel"
                    label: qsTr("Text")
                    description: root.creator.textHint
                    onClicked: root.pick(false)
                }
                ChoiceCard {
                    width: parent.width
                    checked: root.creator.voice
                    glyph: "voiceChannel"
                    label: qsTr("Voice")
                    description: root.creator.voiceHint
                    onClicked: root.pick(true)
                }
            }

            Column {
                width: parent.width
                spacing: 6
                Text {
                    textFormat: Text.PlainText
                    text: qsTr("Channel name")
                    color: Theme.ink1
                    font: Theme.type.labelStrong
                    Accessible.ignored: true
                }
                MobileField {
                    id: nameField
                    width: parent.width
                    leftPadding: 40
                    maximumLength: 100
                    placeholderText: root.creator.placeholder
                    invalid: root.creator.nameError.length > 0
                    enabled: !root.creator.busy
                    inputMethodHints: Qt.ImhNoPredictiveText
                    Accessible.name: qsTr("Channel name")
                    onTextEdited: {
                        const clean = root.creator.clean(text)
                        if (clean !== text) {
                            const at = cursorPosition
                            text = clean
                            cursorPosition = Math.min(at, clean.length)
                        }
                        root.creator.edited()
                    }
                    onAccepted: root.create()
                    Icon {
                        x: 14
                        anchors.verticalCenter: parent.verticalCenter
                        name: root.creator.voice ? "voiceChannel" : "textChannel"
                        size: Theme.iconSm
                        color: Theme.ink3
                    }
                }
                Error {
                    text: root.creator.nameError
                }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: root.creator.placeLine
                    elide: Text.ElideRight
                    color: Theme.ink3
                    font: Theme.type.description
                }
            }
            Error {
                text: root.creator.error
            }
            KitButton {
                width: parent.width
                size: "lg"
                variant: "primary"
                text: qsTr("Create channel")
                loading: root.creator.busy
                onClicked: root.create()
            }
        }
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

    // A type's card (the desktop dialog's ChoiceCard, touch size): the
    // radio, the glyph and label, the description; the chosen one takes
    // the accent ring.
    component ChoiceCard: T.AbstractButton {
        id: card
        property string glyph
        property string label
        property string description
        implicitHeight: Math.max(Theme.controlLg, cardText.implicitHeight + 28)
        focusPolicy: Qt.StrongFocus
        Accessible.role: Accessible.RadioButton
        Accessible.name: label
        Accessible.description: description
        Accessible.checked: checked
        leftPadding: 14
        rightPadding: 14
        topPadding: 14
        bottomPadding: 14
        background: Rectangle {
            radius: Theme.radius
            color: card.checked ? Theme.accentTint : card.pressed ? Theme.tint1 : "transparent"
            EaseColor on color {}
            border.width: card.checked ? 2 : 1
            border.color: card.checked ? Theme.accentInk : Theme.line
        }
        contentItem: Item {
            implicitHeight: cardText.implicitHeight
            Rectangle {
                id: radio
                y: 2
                width: 18
                height: 18
                radius: 9
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
                        font: Theme.type.setting
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
    }
}
