pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// The widget picker (WidgetPicker.tsx): what "+ Add a widget" opens. The
// catalogue by group (Games, Voice & music, Show off, Social, Yours), each
// widget with its glyph, name, one line and the sizes it takes. A type at
// its cap says "Added" (or "2 of 2"), a full board says so, and Custom
// carries the Loud lock on a free plan: it can still be added, as a draft
// only you see, as on the web.
T.Popup {
    id: picker

    property var catalog: []
    property var categories: []
    property int used: 0
    property int max: 12

    signal picked(string kind)

    width: 400
    // Never taller than the window (less the 8 px margins): a long list
    // scrolls inside, so the wheel moves it and not the board behind.
    height: Math.min(560, (T.Overlay.overlay ? T.Overlay.overlay.height : 600) - 16)
    padding: 0
    margins: 8
    modal: false
    focus: true
    closePolicy: T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutside

    background: Rectangle {
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        FloatShadow {}
        Rectangle { width: parent.width; height: 2; color: Theme.ink1 }
    }

    contentItem: Flickable {
        id: flick
        clip: true
        contentWidth: width
        contentHeight: column.implicitHeight + 24
        boundsBehavior: Flickable.StopAtBounds
        interactive: contentHeight > height
        ScrollBar.vertical: ThinScrollBar {}

        Column {
            id: column
            x: 16
            y: 14
            width: flick.width - 32
            spacing: 14

            Row {
                width: parent.width
                Text {
                    textFormat: Text.PlainText
                    width: parent.width - count.implicitWidth
                    text: qsTr("Add a widget")
                    color: Theme.ink1
                    font: Theme.type.menuTitle
                    Accessible.role: Accessible.Heading
                }
                Text {
                    textFormat: Text.PlainText
                    id: count
                    anchors.baseline: parent.children[0].baseline
                    text: qsTr("%1 of %2 used").arg(picker.used).arg(picker.max)
                    color: Theme.ink3
                    font: Theme.type.numSm
                }
            }

            Repeater {
                model: picker.categories
                delegate: Column {
                    id: group
                    required property var modelData
                    width: column.width
                    spacing: 6
                    readonly property var items: picker.catalog.filter(item => item.category === modelData.id)
                    Text {
                        textFormat: Text.PlainText
                        text: group.modelData.label
                        color: Theme.ink3
                        font: Theme.type.silk
                    }
                    Repeater {
                        model: group.items
                        delegate: Rectangle {
                            id: row
                            required property var modelData
                            readonly property bool full: picker.used >= picker.max
                            readonly property bool capped: modelData.remaining === 0
                            readonly property bool can: !capped
                            width: group.width
                            height: 56
                            radius: Theme.radius
                            color: rowHover.hovered && can ? Theme.tint2 : "transparent"
                            opacity: can ? 1 : 0.55
                            Accessible.role: Accessible.Button
                            Accessible.name: modelData.name + (modelData.locked ? qsTr(", Loud") : "") + (capped ? (full ? qsTr(", board is full") : qsTr(", added")) : "")
                            Accessible.description: modelData.blurb

                            Rectangle {
                                x: 8
                                anchors.verticalCenter: parent.verticalCenter
                                width: 32
                                height: 32
                                radius: Theme.radius
                                color: Theme.tint2
                                Icon {
                                    anchors.centerIn: parent
                                    name: row.modelData.icon
                                    size: Theme.iconSm
                                    color: Theme.ink1
                                }
                            }
                            Column {
                                x: 52
                                anchors.verticalCenter: parent.verticalCenter
                                width: parent.width - 52 - tag.width - 16
                                spacing: 2
                                Row {
                                    spacing: 8
                                    Text {
                                        textFormat: Text.PlainText
                                        text: row.modelData.name
                                        color: Theme.ink1
                                        font: Theme.type.labelStrong
                                    }
                                    Text {
                                        textFormat: Text.PlainText
                                        anchors.baseline: parent.children[0].baseline
                                        text: row.modelData.sizes.join(" ")
                                        color: Theme.ink3
                                        font: Theme.type.silk
                                    }
                                }
                                Text {
                                    width: parent.width
                                    textFormat: Text.PlainText
                                    text: row.modelData.blurb
                                    elide: Text.ElideRight
                                    color: Theme.ink3
                                    font: Theme.type.caption
                                }
                            }
                            Row {
                                id: tag
                                anchors.right: parent.right
                                anchors.rightMargin: 8
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
                                    textFormat: Text.PlainText
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: row.capped ? (row.full ? qsTr("Full") : row.modelData.max > 1 ? qsTr("%1 of %1").arg(row.modelData.max) : qsTr("Added"))
                                        : row.modelData.locked ? qsTr("Loud") : ""
                                    color: row.modelData.locked && !row.capped ? Theme.accentInk : Theme.ink3
                                    font: Theme.type.silk
                                }
                            }
                            HoverHandler { id: rowHover; cursorShape: row.can ? Qt.PointingHandCursor : Qt.ArrowCursor }
                            TapHandler {
                                enabled: row.can
                                onTapped: {
                                    picker.picked(row.modelData.type)
                                    picker.close()
                                }
                            }
                        }
                    }
                }
            }

            Text {
                width: parent.width
                visible: picker.catalog.some(item => item.locked)
                textFormat: Text.PlainText
                text: qsTr("Custom widgets show to others with Yap Loud. You can still build one now: it stays a draft only you see.")
                wrapMode: Text.Wrap
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
    }

    enter: PresenceIn { fromScale: Theme.scalePopover }
    exit: PresenceOut { toScale: Theme.scalePopover }
}
