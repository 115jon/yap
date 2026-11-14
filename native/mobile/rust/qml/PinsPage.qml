pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The open conversation's pinned messages, a page over the pager (the
// desktop's PinsPanel, phone layout), on the shared PinsModel. The floating
// bar (Drawer Plus: a ply-3 plate 8 below the status bar) holds the back
// key, the channel over "Pinned messages" and the count; the pins run under
// it, newest first, one row each with a seam below: the avatar, the name
// and when, the text and a line for its files. A tap shows the message in
// the chat; the pin key unpins it after a confirmation. Loading shows three
// placeholder rows; none says how to pin.
Rectangle {
    id: root

    readonly property bool sheet: false
    property real backProgress: 0

    signal closeRequested()

    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: Math.max(SafeArea.margins.bottom, Native.gestureInset)

    readonly property bool dm: Nav.dmUserId.length > 0

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

    FontMetrics {
        id: lines
        font: Theme.type.message
    }

    PinsModel {
        id: pins
        // The chat under this page lands on it.
        onJumped: root.closeRequested()
    }

    // A message's text as it reads, for the confirmation's quote.
    MessageActionModel { id: plainText }

    // The unpin confirmation (PinModal.tsx).
    property string unpinId: ""
    property string unpinQuote: ""
    function askUnpin(id, quote) {
        unpinId = id
        unpinQuote = quote
        unpinSheet.active = true
        unpinSheet.item.open()
    }

    // The bar: back, the channel over the title, the count.
    Item {
        id: bar
        x: 8 + root.leftInset
        y: root.topInset + 8
        width: parent.width - 16 - root.leftInset - root.rightInset
        height: Math.max(56, Math.ceil(kicker.implicitHeight + title.implicitHeight + 12))
        z: 3

        FloatPlate {
            anchors.fill: parent
        }
        IconKey {
            id: backKey
            anchors.left: parent.left
            anchors.leftMargin: 4
            anchors.verticalCenter: parent.verticalCenter
            iconName: "back"
            tip: qsTr("Back to the conversation")
            onClicked: root.closeRequested()
        }
        Column {
            anchors.left: backKey.right
            anchors.leftMargin: 4
            anchors.right: tally.left
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            Text {
                id: kicker
                width: parent.width
                textFormat: Text.PlainText
                text: root.dm ? Nav.activeChannelName : "#" + Nav.activeChannelName
                elide: Text.ElideRight
                color: Theme.ink3
                font: Theme.type.silk
                Accessible.ignored: true
            }
            Text {
                id: title
                width: parent.width
                textFormat: Text.PlainText
                text: qsTr("Pinned messages")
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.columnTitle
                Accessible.role: Accessible.Heading
                Accessible.name: root.dm ? qsTr("Pinned messages with %1").arg(Nav.activeChannelName)
                    : qsTr("Pinned messages in %1").arg(Nav.activeChannelName)
            }
        }
        Text {
            id: tally
            anchors.right: parent.right
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            visible: pins.ready
            textFormat: Text.PlainText
            text: pins.count
            color: Theme.ink3
            font: Theme.type.numSm
            Accessible.name: pins.count === 1 ? qsTr("1 pinned message") : qsTr("%1 pinned messages").arg(pins.count)
        }
    }

    // A solid band under the status bar: the rows scroll behind the bar's
    // plate, never behind the clock and the system's icons.
    Rectangle {
        width: parent.width
        height: root.topInset
        z: 2
        color: Theme.ply0
    }

    readonly property real listTop: bar.y + bar.height + 8

    PlatformList {
        id: list
        anchors.fill: parent
        anchors.leftMargin: root.leftInset
        anchors.rightMargin: root.rightInset
        topMargin: root.listTop
        bottomMargin: root.bottomInset + 16
        visible: pins.ready
        model: pins.rows
        Accessible.role: Accessible.List
        Accessible.name: qsTr("Pinned messages")

        delegate: Rectangle {
            id: row

            required property string messageId
            required property string author
            required property string initials
            required property string avatar
            required property string body
            required property string html
            required property string time
            required property string date
            required property int day
            required property string attachments

            readonly property var mediaItems: pins.mediaOf(attachments)
            readonly property bool hasText: body.trim().length > 0

            width: ListView.view.width
            implicitHeight: Math.max(Theme.hitMin, column.y + column.height + 12) + 1
            color: tap.pressed ? Theme.tint1 : "transparent"
            Behavior on color { ColorAnimation { duration: Theme.dur1 } }
            Accessible.role: Accessible.Button
            Accessible.name: pins.label(qsTr("Jump to message"), author, body, mediaItems.length > 0)
            Accessible.onPressAction: pins.jump(messageId)

            TapHandler {
                id: tap
                gesturePolicy: TapHandler.ReleaseWithinBounds
                onTapped: {
                    Native.haptic(Native.Tick)
                    pins.jump(row.messageId)
                }
            }

            Avatar {
                x: 16
                y: 12
                size: 36
                initials: row.initials
                source: row.avatar
                Accessible.ignored: true
            }

            Column {
                id: column
                x: 16 + 36 + 12
                y: 12
                width: row.width - x - (unpinKey.visible ? unpinKey.width + 8 : 16)
                spacing: 2

                Row {
                    width: parent.width
                    spacing: 8
                    Text {
                        id: name
                        width: Math.min(implicitWidth, parent.width - stamp.implicitWidth - 8)
                        textFormat: Text.PlainText
                        text: row.author
                        elide: Text.ElideRight
                        color: Theme.ink1
                        font: Theme.type.author
                    }
                    Text {
                        id: stamp
                        anchors.baseline: name.baseline
                        textFormat: Text.PlainText
                        text: row.day === Chat.today ? row.time : row.date
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                }
                // The message as it reads, up to six lines (the chat has it
                // whole: a tap goes there). Links stay text here.
                Text {
                    visible: row.hasText
                    width: parent.width
                    // Rich text does not elide: cut at six lines.
                    height: visible ? Math.min(implicitHeight, Math.ceil(lines.lineSpacing * 6)) : 0
                    textFormat: Text.RichText
                    text: Theme.richText(row.html, false)
                    wrapMode: Text.Wrap
                    clip: true
                    color: Theme.ink1
                    font: Theme.type.message
                    Accessible.ignored: true
                }
                // Its files, counted.
                Row {
                    visible: row.mediaItems.length > 0
                    spacing: 6
                    topPadding: 2
                    Icon {
                        anchors.verticalCenter: parent.verticalCenter
                        name: "paperclip"
                        size: Theme.iconSm
                        color: Theme.ink3
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: row.mediaItems.length === 1 ? qsTr("1 attachment") : qsTr("%1 attachments").arg(row.mediaItems.length)
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                }
            }

            // Unpin, lit as the pin it is.
            IconKey {
                id: unpinKey
                anchors.right: parent.right
                anchors.rightMargin: 4
                y: 4
                visible: pins.canUnpin
                iconName: "pin"
                tone: "toggled"
                tip: qsTr("Unpin")
                Accessible.name: pins.label(qsTr("Unpin message"), row.author, row.body, row.mediaItems.length > 0)
                onClicked: root.askUnpin(row.messageId, plainText.plain(row.html))
            }

            Rectangle {
                anchors.bottom: parent.bottom
                x: 16
                width: parent.width - 16
                height: 1
                color: Theme.line
            }
        }
    }

    // Loading: the row's anatomy (the avatar, a name line, two lines).
    Column {
        visible: pins.loading
        x: root.leftInset
        y: root.listTop
        width: parent.width - root.leftInset - root.rightInset
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("Loading")
        Repeater {
            model: 3
            delegate: Item {
                id: skeleton
                required property int index
                width: parent.width
                height: 84
                Rectangle {
                    x: 16
                    y: 12
                    width: 36
                    height: 36
                    radius: Theme.radius
                    color: Theme.tint2
                }
                Column {
                    x: 16 + 36 + 12
                    y: 16
                    width: parent.width - x - 16
                    spacing: 8
                    Rectangle { width: 112; height: 12; radius: Theme.radius; color: Theme.tint2 }
                    Rectangle { width: parent.width; height: 12; radius: Theme.radius; color: Theme.tint2 }
                    Rectangle {
                        width: skeleton.index === 1 ? parent.width / 3 : parent.width * 2 / 3
                        height: 12
                        radius: Theme.radius
                        color: Theme.tint2
                    }
                }
                Rectangle {
                    anchors.bottom: parent.bottom
                    x: 16
                    width: parent.width - 16
                    height: 1
                    color: Theme.line
                }
            }
        }
    }

    // None: the "empty" glyph, the title and how to pin.
    Column {
        visible: pins.empty
        x: 32 + root.leftInset
        y: root.listTop + 48
        width: parent.width - 64 - root.leftInset - root.rightInset
        spacing: 4
        Accessible.role: Accessible.StaticText
        Accessible.name: pins.emptyTitle
        Item {
            width: parent.width
            height: 60 + 12
            Image {
                anchors.horizontalCenter: parent.horizontalCenter
                width: 26 * 3
                height: 20 * 3
                smooth: false
                source: "image://dither/empty/" + Theme.hex(Theme.ink3) + "/" + Theme.hex(Theme.accentInk)
                Accessible.ignored: true
            }
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            textFormat: Text.PlainText
            text: pins.emptyTitle
            color: Theme.ink1
            font: Theme.type.sectionTitle
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            textFormat: Text.PlainText
            text: qsTr("Long-press a message and choose Pin message, and it shows up here for everyone in the conversation.")
            wrapMode: Text.Wrap
            color: Theme.ink2
            font: Theme.type.description
        }
    }

    Loader {
        id: unpinSheet
        anchors.fill: parent
        z: 5
        active: false
        sourceComponent: ConfirmSheet {
            title: pins.confirmTitle(false)
            body: pins.confirmBody(false)
            quote: root.unpinQuote
            confirmText: pins.confirmKey(false)
            onConfirmed: {
                Native.haptic(Native.Confirm)
                pins.unpin(root.unpinId)
            }
        }
    }
}
