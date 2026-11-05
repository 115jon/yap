pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// The search combobox (SearchPanel.tsx `field`): one line, 32 high, ply-2
// on a line-field edge that turns focus ink with a ring. The search glyph,
// then the chips and the input scrolling sideways together, then a fixed
// clear key. The store owns the text's rules; the input reports edits,
// the caret and keys, and takes the text back when the store rewrites it.
Rectangle {
    id: root

    // The panel's parsed state.
    required property var view

    signal closeRequested()
    // Arrow down with results showing and no suggestions.
    signal resultsRequested()

    readonly property bool hasInput: input.text.length > 0 || (view.chips ?? []).length > 0

    function focusInput() {
        input.forceActiveFocus()
    }

    implicitHeight: Theme.keySm
    radius: Theme.radius
    color: Theme.ply2
    border.width: input.activeFocus ? 2 : 1
    border.color: input.activeFocus ? Theme.focus : fieldHover.hovered ? Theme.ink2 : Theme.lineField
    Behavior on border.color { ColorAnimation { duration: Theme.dur1 } }

    // The store rewrote the field (a chip, a pick, a clear): take it.
    property int revision: -1
    onViewChanged: {
        if (view.revision === undefined || view.revision === revision)
            return
        revision = view.revision
        if (input.text !== view.text)
            input.text = view.text
        input.cursorPosition = Math.min(view.caret, input.text.length)
    }

    HoverHandler { id: fieldHover; cursorShape: Qt.IBeamCursor }
    TapHandler { onTapped: input.forceActiveFocus() }

    Icon {
        id: glyph
        x: 8
        anchors.verticalCenter: parent.verticalCenter
        name: "search"
        size: Theme.iconSm
        color: Theme.ink3
    }

    Flickable {
        id: scroller
        anchors.left: glyph.right
        anchors.leftMargin: 6
        anchors.right: clearKey.visible ? clearKey.left : parent.right
        anchors.rightMargin: clearKey.visible ? 2 : 6
        height: parent.height
        contentWidth: line.width
        contentHeight: height
        flickableDirection: Flickable.HorizontalFlick
        boundsBehavior: Flickable.StopAtBounds
        clip: true
        interactive: contentWidth > width

        // A new chip pushes the input right: keep the caret in view.
        function toEnd() {
            contentX = Math.max(0, contentWidth - width)
        }

        Row {
            id: line
            height: scroller.height
            spacing: 4
            onWidthChanged: scroller.toEnd()

            Row {
                id: chipRow
                anchors.verticalCenter: parent.verticalCenter
                visible: (root.view.chips ?? []).length > 0
                spacing: 4
                Repeater {
                    model: root.view.chips ?? []
                    delegate: SearchChip {
                        required property var modelData
                        y: (chipRow.height - height) / 2
                        chip: modelData
                        onRemoveRequested: {
                            Search.removeFilter(modelData.id)
                            input.forceActiveFocus()
                        }
                    }
                }
            }

            TextInput {
                id: input
                anchors.verticalCenter: parent.verticalCenter
                width: Math.max(48, scroller.width - (chipRow.visible ? chipRow.width + line.spacing : 0))
                color: Theme.ink1
                selectionColor: Theme.selection
                selectedTextColor: Theme.ink1
                font: Theme.type.row
                selectByMouse: true
                clip: true
                Accessible.role: Accessible.EditableText
                Accessible.name: qsTr("Search messages in %1").arg(root.view.scopeLabel ?? "")
                Accessible.description: qsTr("Type from:, mentions:, in:, has:, before:, after:, during: or pinned: to add a filter.")

                onTextEdited: Search.edit(text, cursorPosition, Date.now())
                onCursorPositionChanged: Search.setCaret(cursorPosition)
                onActiveFocusChanged: Search.setFocused(activeFocus)

                Keys.onPressed: event => {
                    if (input.inputMethodComposing)
                        return
                    const v = root.view
                    const open = v.suggestionsOpen === true
                    const lit = open && (v.activeId ?? "").length > 0
                    switch (event.key) {
                    case Qt.Key_Down:
                    case Qt.Key_Up:
                        event.accepted = true
                        if (open)
                            Search.moveActive(event.key === Qt.Key_Down ? 1 : -1)
                        else if (event.key === Qt.Key_Down && (v.results ?? []).length > 0)
                            root.resultsRequested()
                        return
                    case Qt.Key_Return:
                    case Qt.Key_Enter:
                        event.accepted = true
                        if (lit)
                            Search.pick(v.activeId, Date.now())
                        else
                            Search.submit(Date.now())
                        return
                    case Qt.Key_Tab:
                        if (lit && !(event.modifiers & Qt.ShiftModifier)) {
                            event.accepted = true
                            Search.pick(v.activeId, Date.now())
                        }
                        return
                    case Qt.Key_Escape:
                        event.accepted = true
                        if (open)
                            Search.dismiss()
                        else
                            root.closeRequested()
                        return
                    case Qt.Key_Backspace:
                        if (input.cursorPosition === 0 && input.selectedText.length === 0
                                && (v.chips ?? []).length > 0) {
                            event.accepted = true
                            Search.removeLastFilter()
                        }
                        return
                    }
                }

                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width
                    visible: input.text.length === 0 && !input.inputMethodComposing
                    textFormat: Text.PlainText
                    text: root.view.placeholder ?? ""
                    elide: Text.ElideRight
                    color: Theme.ink3
                    font: Theme.type.row
                    Accessible.ignored: true
                }
            }
        }
    }

    IconKey {
        id: clearKey
        anchors.right: parent.right
        anchors.rightMargin: 4
        anchors.verticalCenter: parent.verticalCenter
        visible: root.hasInput
        iconName: "close"
        size: Theme.keyXs
        tone: "muted"
        tip: qsTr("Clear search")
        tipSide: "bottom"
        onClicked: {
            Search.clear()
            input.forceActiveFocus()
        }
    }
}
