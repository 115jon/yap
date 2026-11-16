pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import Yap.Ui

// "Add a widget" on the phone (the desktop's BoardPicker as a sheet, on the
// same Yap.Ui BoardPickerCore): the catalogue by group, each widget with
// its glyph, name, the sizes it takes and one line. A type at its cap says
// "Added" (or "2 of 2") and can't be picked, a full board says so, and
// Custom carries the Loud lock on a free plan: it can still be added, as a
// draft only you see. A pick adds the widget at the end and closes.
BottomSheet {
    id: sheet

    property var catalog: []
    property var categories: []
    property int used: 0
    property int max: 12

    signal picked(string kind)

    readonly property BoardPickerCore core: BoardPickerCore {
        catalog: sheet.catalog
        categories: sheet.categories
        used: sheet.used
        max: sheet.max
    }

    title: qsTr("Add a widget")
    contentHeight: column.implicitHeight
    flickable: flick

    Flickable {
        id: flick
        anchors.fill: parent
        contentHeight: column.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickDeceleration: Native.flingDeceleration
        maximumFlickVelocity: Native.maxFlingVelocity

        Column {
            id: column
            width: flick.width
            bottomPadding: 16
            spacing: 4

            Text {
                x: 16
                width: parent.width - 32
                bottomPadding: 4
                textFormat: Text.PlainText
                text: sheet.core.full ? qsTr("Your board is full: remove a widget to add another.")
                    : qsTr("%1 of %2 used").arg(sheet.used).arg(sheet.max)
                wrapMode: Text.Wrap
                color: Theme.ink3
                font: sheet.core.full ? Theme.type.description : Theme.type.numSm
            }

            Repeater {
                model: sheet.categories
                delegate: Column {
                    id: group
                    required property var modelData
                    readonly property var items: sheet.core.itemsOf(modelData.id)
                    width: column.width
                    visible: items.length > 0
                    topPadding: 8
                    Text {
                        x: 16
                        bottomPadding: 4
                        textFormat: Text.PlainText
                        text: group.modelData.label
                        color: Theme.ink3
                        font: Theme.type.silk
                        Accessible.role: Accessible.Heading
                    }
                    Repeater {
                        model: group.items
                        delegate: Item {
                            id: row
                            required property var modelData
                            readonly property bool can: sheet.core.canAdd(modelData)
                            readonly property string tag: sheet.core.tag(modelData)
                            width: group.width
                            height: Math.max(64, labels.implicitHeight + 20)
                            opacity: can ? 1 : 0.55
                            Accessible.role: Accessible.Button
                            Accessible.name: sheet.core.spokenName(modelData)
                            Accessible.description: modelData.blurb
                            Accessible.onPressAction: row.pick()
                            function pick() {
                                if (!can || sheet.closing)
                                    return
                                Native.haptic(Native.Tick)
                                sheet.picked(modelData.type)
                                sheet.close()
                            }

                            Rectangle {
                                anchors.fill: parent
                                color: tap.pressed && row.can ? Theme.tint2 : "transparent"
                            }
                            Rectangle {
                                id: glyph
                                x: 16
                                anchors.verticalCenter: parent.verticalCenter
                                width: 40
                                height: 40
                                radius: Theme.radius
                                color: Theme.tint2
                                Icon {
                                    anchors.centerIn: parent
                                    name: row.modelData.icon
                                    size: Theme.iconMd
                                    color: Theme.ink1
                                }
                            }
                            Column {
                                id: labels
                                anchors.left: glyph.right
                                anchors.leftMargin: 12
                                anchors.right: tagRow.left
                                anchors.rightMargin: 8
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 2
                                Row {
                                    spacing: 8
                                    Text {
                                        id: nameText
                                        textFormat: Text.PlainText
                                        text: row.modelData.name
                                        color: Theme.ink1
                                        font: Theme.type.labelStrong
                                    }
                                    Text {
                                        anchors.baseline: nameText.baseline
                                        textFormat: Text.PlainText
                                        text: row.modelData.sizes.join(" ")
                                        color: Theme.ink3
                                        font: Theme.type.silk
                                    }
                                }
                                Text {
                                    width: parent.width
                                    textFormat: Text.PlainText
                                    text: row.modelData.blurb
                                    wrapMode: Text.Wrap
                                    maximumLineCount: 2
                                    elide: Text.ElideRight
                                    color: Theme.ink3
                                    font: Theme.type.caption
                                }
                            }
                            Row {
                                id: tagRow
                                anchors.right: parent.right
                                anchors.rightMargin: 16
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 4
                                Icon {
                                    visible: row.modelData.locked
                                    anchors.verticalCenter: parent.verticalCenter
                                    name: "lock"
                                    size: Theme.iconXs
                                    color: Theme.accentInk
                                }
                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    visible: row.tag.length > 0
                                    textFormat: Text.PlainText
                                    text: row.tag
                                    color: row.modelData.locked && row.can ? Theme.accentInk : Theme.ink3
                                    font: Theme.type.silk
                                }
                            }
                            TapHandler {
                                id: tap
                                onTapped: row.pick()
                            }
                        }
                    }
                }
            }

            Text {
                x: 16
                width: parent.width - 32
                topPadding: 8
                visible: sheet.core.anyLocked
                textFormat: Text.PlainText
                text: qsTr("Custom widgets show to others with Yap Loud. You can still build one now: it stays a draft only you see.")
                wrapMode: Text.Wrap
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
    }
}
