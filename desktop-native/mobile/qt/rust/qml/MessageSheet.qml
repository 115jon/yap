pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import Yap.Ui

// The message menu on a phone (the desktop's MessageMenu and hover bar): a
// preview of the message, a row of quick reactions with "+" for the whole
// picker, then the actions Yap.Ui's MessageActionModel offers for it. It
// only says what was picked; MessageActions carries each one out.
BottomSheet {
    id: sheet

    // The message it is open on, a snapshot of its row (MessageActions).
    property var message: ({})
    // Its actions in groups, a seam above each (MessageActionModel
    // `groups("sheet")`).
    property var groups: []
    readonly property var quick: JSON.parse(Emoji.quick)
    readonly property string preview: message.preview ?? ""

    signal reactionPicked(string emoji)
    signal morePicked()
    // A MessageActionModel action.
    signal actionPicked(string action)

    function pick(action) {
        close()
        actionPicked(action)
    }

    contentHeight: column.implicitHeight
    flickable: flick

    Flickable {
        id: flick
        anchors.fill: parent
        contentHeight: column.implicitHeight
        clip: true

        Column {
            id: column
            width: flick.width

            // Whose message, when, and its first lines.
            Item {
                width: parent.width
                height: Math.max(40, preview.implicitHeight) + 12
                Avatar {
                    id: face
                    x: 16
                    size: 40
                    initials: sheet.message.initials ?? ""
                    source: sheet.message.avatar ?? ""
                }
                Column {
                    id: preview
                    anchors.left: face.right
                    anchors.leftMargin: 12
                    anchors.right: parent.right
                    anchors.rightMargin: 16
                    Row {
                        spacing: 8
                        Text {
                            id: name
                            width: Math.min(implicitWidth, preview.width - when.implicitWidth - 8)
                            text: sheet.message.author ?? ""
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                            color: Theme.ink1
                            font: Theme.type.author
                        }
                        Text {
                            id: when
                            anchors.baseline: name.baseline
                            text: sheet.message.time ?? ""
                            textFormat: Text.PlainText
                            color: Theme.ink3
                            font: Theme.type.numSm
                        }
                    }
                    Text {
                        width: parent.width
                        visible: sheet.preview.length > 0
                        text: sheet.preview
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                        color: Theme.ink2
                        font: Theme.type.description
                    }
                }
            }

            // Quick reactions: the recents, then the defaults; "+" last.
            Row {
                id: quickRow
                readonly property real cell: Math.floor((column.width - 32 - spacing * 5) / 6)
                x: 16
                topPadding: 4
                bottomPadding: 12
                spacing: 8
                Repeater {
                    model: sheet.quick.slice(0, 5)
                    Rectangle {
                        id: chip
                        required property var modelData
                        width: quickRow.cell
                        height: Theme.controlLg
                        radius: Theme.radius
                        color: chipTap.pressed ? Theme.tint3 : Theme.tint1
                        scale: chipTap.pressed ? Theme.scalePressIcon : 1
                        Accessible.role: Accessible.Button
                        Accessible.name: qsTr("React with %1").arg(":" + chip.modelData.s + ":")
                        Accessible.onPressAction: chip.pick()
                        function pick() {
                            sheet.close()
                            sheet.reactionPicked(chip.modelData.e)
                        }
                        EmojiGlyph {
                            anchors.centerIn: parent
                            size: 26
                            emoji: chip.modelData.e
                            url: chip.modelData.u
                        }
                        TapHandler {
                            id: chipTap
                            onTapped: chip.pick()
                        }
                    }
                }
                Rectangle {
                    id: more
                    width: quickRow.cell
                    height: Theme.controlLg
                    radius: Theme.radius
                    color: moreTap.pressed ? Theme.tint3 : Theme.tint1
                    scale: moreTap.pressed ? Theme.scalePressIcon : 1
                    Accessible.role: Accessible.Button
                    Accessible.name: qsTr("Add reaction")
                    Accessible.onPressAction: more.pick()
                    function pick() {
                        sheet.close()
                        sheet.morePicked()
                    }
                    Icon {
                        anchors.centerIn: parent
                        name: "react"
                        size: Theme.iconXl
                        color: Theme.ink2
                    }
                    TapHandler {
                        id: moreTap
                        onTapped: more.pick()
                    }
                }
            }

            Repeater {
                model: sheet.groups
                Column {
                    id: group
                    required property var modelData
                    width: column.width
                    Seam {}
                    Repeater {
                        model: group.modelData
                        SheetRow {
                            id: actionRow
                            required property var modelData
                            text: actionRow.modelData.text
                            iconName: actionRow.modelData.glyph
                            danger: actionRow.modelData.danger
                            hint: actionRow.modelData.id === "reactions" ? String(sheet.message.reactionCount ?? "") : ""
                            onTriggered: sheet.pick(actionRow.modelData.id)
                        }
                    }
                }
            }
            Item { width: 1; height: 8 }
        }
    }

    // A seam between groups: 1 px, 4 above and below.
    component Seam: Item {
        width: parent ? parent.width : 0
        height: 9
        Rectangle {
            y: 4
            width: parent.width
            height: 1
            color: Theme.line
        }
    }
}
