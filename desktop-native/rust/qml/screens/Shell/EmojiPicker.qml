pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// The emoji picker (EmojiPicker.tsx, PickerSurface.tsx): 420 wide, up to
// 560 high; ply-3 with a line-strong edge and the 2 px ink-1 top rule.
// The header has the search field and the skin tone key; the 40 px rail
// jumps between sections; the grid shows Twemoji at 28 px in 40 px cells,
// nine to a row; the footer names the hovered emoji. Enter in the search
// picks the first result; Esc closes (the tone menu first).
Popup {
    id: root

    // What a pick does: insert into the composer, or react to a message.
    signal picked(string value)

    property var hovered: null
    readonly property var tones: [
        { label: qsTr("Default") }, { label: qsTr("Light") }, { label: qsTr("Medium-Light") },
        { label: qsTr("Medium") }, { label: qsTr("Medium-Dark") }, { label: qsTr("Dark") }
    ]
    readonly property var toneUrls: [
        "1f44f", "1f44f-1f3fb", "1f44f-1f3fc", "1f44f-1f3fd", "1f44f-1f3fe", "1f44f-1f3ff"
    ].map(u => "https://cdn.jsdelivr.net/npm/emoji-datasource-twitter@16.0.0/img/twitter/64/" + u + ".png")
    readonly property var sections: [
        { id: "recent", icon: "clock", label: qsTr("Recently used") },
        { id: "people", icon: "emoji", label: qsTr("Smileys & People") },
        { id: "nature", icon: "nature", label: qsTr("Animals & Nature") },
        { id: "foods", icon: "food", label: qsTr("Food & Drink") },
        { id: "activity", icon: "gamepad", label: qsTr("Activities") },
        { id: "places", icon: "globe", label: qsTr("Travel & Places") },
        { id: "objects", icon: "lightbulb", label: qsTr("Objects") },
        { id: "symbols", icon: "like", label: qsTr("Symbols") },
        { id: "flags", icon: "flag", label: qsTr("Flags") }
    ]
    property string activeSection: "people"

    // Opens next to `anchor` (above it when there is room, else below).
    function openAt(anchor, alignRight) {
        const win = Overlay.overlay
        const at = anchor.mapToItem(win, 0, 0)
        const h = Math.min(560, win.height - 24)
        x = Math.max(10, Math.min(win.width - width - 10, alignRight ? at.x + anchor.width - width : at.x))
        y = at.y - h - 8 >= 10 ? at.y - h - 8 : Math.min(win.height - h - 10, at.y + anchor.height + 8)
        height = h
        open()
    }
    function pick(value) {
        if (value.length === 0)
            return
        Emoji.remember(value)
        root.picked(value)
        root.close()
    }

    parent: Overlay.overlay
    width: Math.min(420, (parent ? parent.width : 420) - 24)
    padding: 0
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onOpened: {
        search.text = ""
        Emoji.search("")
        grid.positionViewAtBeginning()
        search.forceActiveFocus()
    }
    onClosed: hovered = null

    background: Rectangle {
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        Rectangle {
            width: parent.width
            height: 2
            color: Theme.ink1
        }
    }
    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
            NumberAnimation { property: "scale"; from: Theme.scaleIn; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
        }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: Theme.dur3Exit }
    }

    contentItem: Item {
        // Header: search and the tone key.
        Item {
            id: header
            width: parent.width
            height: 48
            Rectangle {
                id: field
                x: 8
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - 16 - 36
                height: 32
                radius: Theme.radius
                color: Theme.ply2
                border.width: 1
                border.color: search.activeFocus ? Theme.ink1 : Theme.lineField
                Icon {
                    id: searchGlyph
                    x: 8
                    anchors.verticalCenter: parent.verticalCenter
                    name: "search"
                    size: Theme.iconSm
                    color: Theme.ink3
                }
                TextField {
                    id: search
                    anchors.left: searchGlyph.right
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    leftPadding: 6
                    rightPadding: 8
                    placeholderText: qsTr("Search emoji")
                    placeholderTextColor: Theme.ink3
                    color: Theme.ink1
                    selectionColor: Theme.selection
                    font: Theme.type.description
                    background: null
                    onTextChanged: {
                        Emoji.search(text)
                        grid.positionViewAtBeginning()
                    }
                    Keys.onReturnPressed: {
                        const first = grid.itemAtIndex(1)
                        if (text.trim().length > 0 && first && first.parsed.length > 0)
                            root.pick(first.parsed[0].e)
                    }
                    Keys.onDownPressed: grid.forceActiveFocus()
                    Keys.onEscapePressed: event => {
                        if (toneMenu.visible) {
                            toneMenu.close()
                            event.accepted = true
                        } else {
                            root.close()
                        }
                    }
                }
            }
            Item {
                id: toneKey
                anchors.right: parent.right
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                width: 32
                height: 32
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Skin tone")
                Rectangle {
                    anchors.fill: parent
                    radius: Theme.radius
                    color: toneHover.hovered || toneMenu.visible ? Theme.tint2 : "transparent"
                }
                Image {
                    anchors.centerIn: parent
                    width: 20
                    height: 20
                    sourceSize: Qt.size(40, 40)
                    asynchronous: true
                    source: root.toneUrls[Emoji.skinTone]
                }
                Tooltip { visible: toneHover.hovered && !toneMenu.visible; text: qsTr("Skin tone") }
                HoverHandler { id: toneHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: toneMenu.visible ? toneMenu.close() : toneMenu.open() }
            }
        }

        Rectangle {
            id: rail
            y: header.height
            width: 40
            height: parent.height - header.height - footer.height
            color: "transparent"
            Rectangle {
                anchors.right: parent.right
                width: 1
                height: parent.height
                color: Theme.line
            }
            Column {
                x: 4
                y: 4
                spacing: 4
                Repeater {
                    model: root.sections
                    IconKey {
                        required property var modelData
                        size: 32
                        iconName: modelData.icon
                        tip: modelData.label
                        tipSide: "right"
                        tone: root.activeSection === modelData.id ? "toggled" : "muted"
                        onClicked: {
                            search.text = ""
                            const row = Emoji.sectionRow(modelData.id)
                            if (row >= 0)
                                grid.positionViewAtIndex(row, ListView.Beginning)
                        }
                    }
                }
            }
        }

        ListView {
            id: grid
            anchors.left: rail.right
            anchors.right: parent.right
            y: header.height
            height: parent.height - header.height - footer.height
            leftMargin: 4
            rightMargin: 4
            clip: true
            model: EmojiGridModel
            reuseItems: true
            boundsBehavior: Flickable.StopAtBounds
            keyNavigationEnabled: false
            onContentYChanged: {
                const top = itemAt(width / 2, contentY + 33)
                if (top && top.section !== "results")
                    root.activeSection = top.section
            }
            ScrollBar.vertical: ScrollBar {
                policy: ScrollBar.AsNeeded
                contentItem: Rectangle {
                    implicitWidth: 6
                    radius: 3
                    color: Theme.lineStrong
                }
            }

            delegate: Item {
                id: gridRow
                required property string kind
                required property string label
                required property string section
                required property string cells
                readonly property var parsed: kind === "cells" ? JSON.parse(cells) : []
                width: grid.width - 8
                height: kind === "header" ? 32 : 40

                Text {
                    visible: gridRow.kind === "header"
                    x: 6
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - 12
                    elide: Text.ElideRight
                    text: gridRow.label
                    color: Theme.ink3
                    font: Theme.type.captionStrong
                    Accessible.role: Accessible.Heading
                }
                Row {
                    visible: gridRow.kind === "cells"
                    Repeater {
                        model: gridRow.parsed
                        Item {
                            id: cell
                            required property var modelData
                            width: 40
                            height: 40
                            Accessible.role: Accessible.Button
                            Accessible.name: ":" + cell.modelData.s + ":"
                            Rectangle {
                                anchors.fill: parent
                                radius: Theme.radius
                                color: cellTap.pressed ? Theme.tint3 : cellHover.hovered ? Theme.tint2 : "transparent"
                            }
                            Image {
                                anchors.centerIn: parent
                                width: 28
                                height: 28
                                sourceSize: Qt.size(56, 56)
                                asynchronous: true
                                cache: true
                                smooth: true
                                mipmap: true
                                source: cell.modelData.u
                            }
                            HoverHandler {
                                id: cellHover
                                cursorShape: Qt.PointingHandCursor
                                onHoveredChanged: if (hovered) root.hovered = cell.modelData
                            }
                            TapHandler { id: cellTap; onTapped: root.pick(cell.modelData.e) }
                        }
                    }
                }
            }
        }

        // Footer: the hovered emoji, its name and `:code:`.
        Item {
            id: footer
            anchors.bottom: parent.bottom
            width: parent.width
            height: 40
            Rectangle {
                width: parent.width
                height: 1
                color: Theme.line
            }
            Image {
                id: preview
                x: 12
                anchors.verticalCenter: parent.verticalCenter
                visible: root.hovered !== null
                width: 24
                height: 24
                sourceSize: Qt.size(48, 48)
                source: root.hovered ? root.hovered.u : ""
            }
            Text {
                anchors.left: preview.visible ? preview.right : parent.left
                anchors.leftMargin: 12
                anchors.right: code.left
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                elide: Text.ElideRight
                text: root.hovered ? root.hovered.n : qsTr("Pick an emoji")
                color: root.hovered ? Theme.ink1 : Theme.ink3
                font: Theme.type.occupant
            }
            Text {
                id: code
                anchors.right: parent.right
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                text: root.hovered ? ":" + root.hovered.s + ":" : ""
                color: Theme.ink3
                font: Theme.type.numSm
            }
        }

        // The tone menu: 208 wide, under the key, right-aligned.
        Popup {
            id: toneMenu
            x: parent.width - width - 8
            y: header.height - 4
            width: 208
            padding: 4
            closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
            background: Rectangle {
                radius: Theme.radius
                color: Theme.ply3
                border.width: 1
                border.color: Theme.lineStrong
            }
            contentItem: Column {
                Repeater {
                    model: root.tones.length
                    Rectangle {
                        id: toneRow
                        required property int index
                        width: 200
                        height: 32
                        radius: Theme.radius
                        color: toneRowHover.hovered ? Theme.tint2 : "transparent"
                        Accessible.role: Accessible.MenuItem
                        Accessible.name: root.tones[index].label
                        Image {
                            id: toneGlyph
                            x: 8
                            anchors.verticalCenter: parent.verticalCenter
                            width: 16
                            height: 16
                            sourceSize: Qt.size(32, 32)
                            source: root.toneUrls[toneRow.index]
                        }
                        Text {
                            anchors.left: toneGlyph.right
                            anchors.leftMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            text: root.tones[toneRow.index].label
                            color: Theme.ink1
                            font: Theme.type.row
                        }
                        Icon {
                            anchors.right: parent.right
                            anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            visible: Emoji.skinTone === toneRow.index
                            name: "check"
                            size: Theme.iconSm
                            color: Theme.ink1
                        }
                        HoverHandler { id: toneRowHover; cursorShape: Qt.PointingHandCursor }
                        TapHandler {
                            onTapped: {
                                Emoji.setTone(toneRow.index)
                                toneMenu.close()
                            }
                        }
                    }
                }
            }
        }
    }
}
