pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import Yap.Mobile
import Yap.Ui

// The emoji picker on a phone: the desktop picker's data (the `Emoji`
// singleton and EmojiGridModel: recents, the person's own emoji, every
// section, search by name, the skin tone) in a sheet. A search field and
// the skin tone key on top; rows of nine cells under silkscreen section
// labels. It says what was picked and closes; the caller reacts or inserts.
BottomSheet {
    id: sheet

    signal picked(string emoji)

    readonly property var toneNames: [
        qsTr("Default"), qsTr("Light"), qsTr("Medium-light"),
        qsTr("Medium"), qsTr("Medium-dark"), qsTr("Dark")
    ]
    // The clapping hands in each tone, as the desktop's tone key shows.
    readonly property var toneGlyphs: ["\u{1F44F}", "\u{1F44F}\u{1F3FB}", "\u{1F44F}\u{1F3FC}",
                                       "\u{1F44F}\u{1F3FD}", "\u{1F44F}\u{1F3FE}", "\u{1F44F}\u{1F3FF}"]
    readonly property var toneUrls: ["1f44f", "1f44f-1f3fb", "1f44f-1f3fc", "1f44f-1f3fd", "1f44f-1f3fe", "1f44f-1f3ff"]
        .map(u => "https://cdn.jsdelivr.net/npm/emoji-datasource-twitter@16.0.0/img/twitter/64/" + u + ".png")
    property bool tonesOpen: false

    function openPicker() {
        tonesOpen = false
        search.text = ""
        Emoji.setCreations(Extras.emojis)
        Emoji.search("")
        grid.positionViewAtBeginning()
        open()
    }

    // The grid is long: the sheet rests at its detent and pulls up to full.
    contentHeight: 2000
    flickable: grid
    onClosed: search.focus = false

    // The search field (KitField's plate) and the skin tone key.
    Item {
        id: head
        width: parent.width
        height: Theme.controlLg + 8 + (sheet.tonesOpen ? Theme.controlLg + 8 : 0)

        Rectangle {
            id: plate
            x: 16
            width: parent.width - 32 - Theme.controlLg - 8
            height: Theme.controlLg
            radius: Theme.radius
            color: Theme.ply2
            border.width: 1
            border.color: search.activeFocus ? Theme.ink1 : Theme.lineField

            Icon {
                id: glass
                x: 14
                anchors.verticalCenter: parent.verticalCenter
                name: "search"
                size: Theme.iconMd
                color: Theme.ink3
            }
            TextField {
                id: search
                anchors.left: glass.right
                anchors.leftMargin: 10
                anchors.right: clear.visible ? clear.left : parent.right
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                background: null
                padding: 0
                color: Theme.ink1
                placeholderText: qsTr("Search emoji")
                placeholderTextColor: Theme.ink3
                selectionColor: Theme.selection
                selectedTextColor: Theme.ink1
                font: Theme.type.message
                inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText
                EnterKey.type: Qt.EnterKeySearch
                Accessible.name: qsTr("Search emoji")
                // The word in progress counts: the IME has not committed it.
                readonly property string query: text + preeditText
                onQueryChanged: {
                    Emoji.search(query.trim())
                    grid.positionViewAtBeginning()
                }
                onAccepted: Qt.inputMethod.hide()
            }
            IconKey {
                id: clear
                visible: search.query.length > 0
                anchors.right: parent.right
                anchors.rightMargin: 4
                anchors.verticalCenter: parent.verticalCenter
                size: 32
                tone: "muted"
                iconName: "close"
                tip: qsTr("Clear search")
                onClicked: {
                    Qt.inputMethod.reset()
                    search.clear()
                }
            }
        }
        Rectangle {
            id: toneKey
            anchors.left: plate.right
            anchors.leftMargin: 8
            width: Theme.controlLg
            height: Theme.controlLg
            radius: Theme.radius
            color: sheet.tonesOpen ? Theme.tint2 : toneTap.pressed ? Theme.tint3 : Theme.tint1
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Choose emoji skin tone")
            EmojiGlyph {
                anchors.centerIn: parent
                size: 24
                emoji: sheet.toneGlyphs[Emoji.skinTone] ?? sheet.toneGlyphs[0]
                url: sheet.toneUrls[Emoji.skinTone] ?? sheet.toneUrls[0]
            }
            TapHandler {
                id: toneTap
                onTapped: sheet.tonesOpen = !sheet.tonesOpen
            }
        }
        // The six tones, in a row under the field while the key is on.
        Row {
            id: tones
            visible: sheet.tonesOpen
            x: 16
            y: Theme.controlLg + 8
            spacing: 8
            Repeater {
                model: 6
                Rectangle {
                    id: tone
                    required property int index
                    width: Math.floor((head.width - 32 - 5 * 8) / 6)
                    height: Theme.controlLg
                    radius: Theme.radius
                    color: Emoji.skinTone === index ? Theme.tint2 : pickTone.pressed ? Theme.tint3 : "transparent"
                    border.width: Emoji.skinTone === index ? 2 : 0
                    border.color: Theme.ink1
                    Accessible.role: Accessible.RadioButton
                    Accessible.name: sheet.toneNames[index]
                    Accessible.checked: Emoji.skinTone === index
                    EmojiGlyph {
                        anchors.centerIn: parent
                        size: 24
                        emoji: sheet.toneGlyphs[tone.index]
                        url: sheet.toneUrls[tone.index]
                    }
                    TapHandler {
                        id: pickTone
                        onTapped: {
                            Native.haptic(Native.Tick)
                            const y = grid.contentY
                            Emoji.setTone(tone.index)
                            grid.contentY = y
                            sheet.tonesOpen = false
                        }
                    }
                }
            }
        }
    }

    ListView {
        id: grid
        anchors.top: head.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        clip: true
        model: EmojiGridModel
        reuseItems: true
        cacheBuffer: 400
        // A scroll puts the keyboard away, as the system pickers do.
        onMovementStarted: Qt.inputMethod.hide()

        readonly property real cell: Math.floor((width - 16) / 9)

        delegate: Item {
            id: gridRow
            required property string kind
            required property string label
            required property string section
            required property string cells
            readonly property var parsed: kind === "cells" ? JSON.parse(cells) : []
            width: grid.width
            height: kind === "header" ? 32 : kind === "cells" ? grid.cell
                : kind === "empty" ? 160 : kind === "error" ? notice.implicitHeight + 24 : 0

            // A section's label, silkscreen; the results' count after it.
            Text {
                visible: gridRow.kind === "header"
                x: 16
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 6
                text: gridRow.section === "results" && gridRow.cells.length > 0
                    ? gridRow.label + "  " + gridRow.cells : gridRow.label
                textFormat: Text.PlainText
                color: Theme.ink3
                font: Theme.type.silk
            }
            Row {
                visible: gridRow.kind === "cells"
                x: 8
                Repeater {
                    model: gridRow.parsed
                    Item {
                        id: cell
                        required property var modelData
                        width: grid.cell
                        height: grid.cell
                        Accessible.role: Accessible.Button
                        Accessible.name: ":" + cell.modelData.s + ":"
                        Rectangle {
                            anchors.fill: parent
                            anchors.margins: 2
                            radius: Theme.radius
                            color: cellTap.pressed ? Theme.tint3 : "transparent"
                        }
                        EmojiGlyph {
                            anchors.centerIn: parent
                            size: Math.min(30, grid.cell - 12)
                            emoji: cell.modelData.e
                            url: cell.modelData.u
                        }
                        TapHandler {
                            id: cellTap
                            onTapped: {
                                // Not ready yet (a creation still generating).
                                if (cell.modelData.e.length === 0)
                                    return
                                Emoji.remember(cell.modelData.e)
                                sheet.close()
                                sheet.picked(cell.modelData.e)
                            }
                        }
                    }
                }
            }
            // No hits, or the person's own emoji failed to load.
            Text {
                id: notice
                visible: gridRow.kind === "empty" || gridRow.kind === "error"
                anchors.fill: parent
                leftPadding: 24
                rightPadding: 24
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: gridRow.label
                color: gridRow.kind === "error" ? Theme.dangerInk : Theme.ink3
                font: Theme.type.description
            }
        }
    }
}
