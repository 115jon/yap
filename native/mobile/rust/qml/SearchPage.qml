pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Message search, a page over the pager (the desktop's SearchPanel, phone
// layout), on the same SearchCore and rows. It searches what is open: the
// server of the channel in view, or the DM. The floating bar (Drawer Plus:
// a ply-3 plate 8 below the status bar) holds the back key and the field,
// whose filters (from:, mentions:, in:, has:, before:, after:, during:,
// pinned:) turn into chips as the store reads them; under it the
// suggestions while a filter is typed, else the idle options and recent
// searches, the count and sort, or the results grouped by channel. A
// result opens its channel on the message (Main: conversationRequested)
// and the search is kept, so the search key brings it back; Back closes
// it.
Rectangle {
    id: root

    readonly property bool sheet: false
    property real backProgress: 0

    signal closeRequested()
    // A result's channel, which the core opens on the message.
    signal conversationRequested(string channelId)

    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: Math.max(SafeArea.margins.bottom, Native.gestureInset, Native.imeHeight)

    readonly property var view: core.view
    // Left for a result: the search stays open for the next visit.
    property bool jumped: false

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

    SearchCore {
        id: core
        onGenerationChanged: body.contentY = -body.topMargin
        onRewritten: (text, caret) => {
            // The store's text replaces what the keyboard was composing.
            if (input.text !== text) {
                Qt.inputMethod.reset()
                input.text = text
            }
            input.cursorPosition = Math.min(caret, input.text.length)
            // A search that starts empty (a new one, a clear) types; one
            // kept from a visit that left for a result is read first.
            if (text.length === 0 && (root.view.chips ?? []).length === 0 && !input.activeFocus)
                Qt.callLater(root.focusInput)
        }
        onJumped: result => {
            root.jumped = true
            Qt.inputMethod.hide()
            root.conversationRequested(result.channelId)
        }
    }

    function focusInput() {
        input.forceActiveFocus()
        Qt.inputMethod.show()
    }

    function toTop() {
        body.contentY = -body.topMargin
        suggestions.contentY = -suggestions.topMargin
    }
    Component.onCompleted: {
        core.open("")
        // Once laid out (the bar's height sets the lists' top margin): a
        // kept search has no new generation to move the list for it.
        Qt.callLater(root.toTop)
    }
    Component.onDestruction: {
        if (!root.jumped)
            Search.close()
    }

    // The live region's line.
    Text {
        width: 0
        height: 0
        opacity: 0
        textFormat: Text.PlainText
        text: root.view.announcement ?? ""
        Accessible.role: Accessible.StaticText
        Accessible.name: text
    }

    // The bar: back, then the field (the search glyph, the chips and the
    // input scrolling sideways together, a clear key).
    Item {
        id: bar
        x: 8 + root.leftInset
        y: root.topInset + 8
        width: parent.width - 16 - root.leftInset - root.rightInset
        height: 56
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
            tip: qsTr("Back")
            onClicked: root.closeRequested()
        }
        Rectangle {
            id: field
            anchors.left: backKey.right
            anchors.leftMargin: 4
            anchors.right: parent.right
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            height: 40
            radius: Theme.radius
            color: Theme.ply2
            border.width: input.activeFocus ? 2 : 1
            border.color: input.activeFocus ? Theme.ink1 : Theme.lineField
            Behavior on border.color { ColorAnimation { duration: Theme.dur1 } }

            TapHandler { onTapped: root.focusInput() }

            Icon {
                id: glyph
                x: 10
                anchors.verticalCenter: parent.verticalCenter
                name: "search"
                size: Theme.iconSm
                color: Theme.ink3
            }

            Flickable {
                id: scroller
                anchors.left: glyph.right
                anchors.leftMargin: 8
                anchors.right: clearKey.visible ? clearKey.left : parent.right
                anchors.rightMargin: clearKey.visible ? 0 : 10
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
                                    root.focusInput()
                                }
                            }
                        }
                    }

                    TextInput {
                        id: input
                        anchors.verticalCenter: parent.verticalCenter
                        width: Math.max(64, scroller.width - (chipRow.visible ? chipRow.width + line.spacing : 0))
                        color: Theme.ink1
                        selectionColor: Theme.selection
                        selectedTextColor: Theme.ink1
                        font: Theme.type.message
                        clip: true
                        // Each letter reaches the store as it is typed.
                        inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText
                        EnterKey.type: Qt.EnterKeySearch
                        Accessible.role: Accessible.EditableText
                        Accessible.name: qsTr("Search messages in %1").arg(root.view.scopeLabel ?? "")
                        Accessible.description: qsTr("Type from:, mentions:, in:, has:, before:, after:, during: or pinned: to add a filter.")

                        // What the keyboard still composes counts too (some
                        // keyboards compose even without suggestions).
                        readonly property string live: text.slice(0, cursorPosition) + preeditText + text.slice(cursorPosition)
                        onTextEdited: core.edit(text, cursorPosition)
                        onPreeditTextChanged: core.edit(live, cursorPosition + preeditText.length)
                        onCursorPositionChanged: Search.setCaret(cursorPosition)
                        onActiveFocusChanged: Search.setFocused(activeFocus)
                        onAccepted: core.enter()
                        Keys.onPressed: event => {
                            // Backspace at the start takes the last chip.
                            if (event.key === Qt.Key_Backspace && !input.inputMethodComposing
                                    && input.cursorPosition === 0 && input.selectedText.length === 0
                                    && (root.view.chips ?? []).length > 0) {
                                event.accepted = true
                                Search.removeLastFilter()
                            }
                        }

                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            width: parent.width
                            visible: input.text.length === 0 && input.preeditText.length === 0
                            textFormat: Text.PlainText
                            text: root.view.placeholder ?? ""
                            elide: Text.ElideRight
                            color: Theme.ink3
                            font: Theme.type.message
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
                visible: core.hasInput || input.text.length > 0 || input.preeditText.length > 0
                iconName: "close"
                size: Theme.keySm
                tone: "muted"
                tip: qsTr("Clear search")
                onClicked: {
                    Qt.inputMethod.reset()
                    Search.clear()
                    root.focusInput()
                }
            }
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

    // Inline suggestions replace the body while a filter is typed.
    Flickable {
        id: suggestions
        anchors.fill: parent
        topMargin: bar.y + bar.height + 8
        bottomMargin: root.bottomInset + 16
        onTopMarginChanged: contentY = -topMargin
        visible: core.suggestionsShown
        contentHeight: suggestionColumn.height
        boundsBehavior: Flickable.StopAtBounds
        Accessible.role: Accessible.List
        Accessible.name: qsTr("Search suggestions")

        Column {
            id: suggestionColumn
            x: 8 + root.leftInset
            width: parent.width - 16 - root.leftInset - root.rightInset
            Repeater {
                model: core.suggestionRows
                delegate: Loader {
                    id: suggestionRow
                    required property var modelData
                    width: suggestionColumn.width
                    sourceComponent: modelData.heading !== undefined ? suggestionHeading : suggestionOption
                    Component {
                        id: suggestionOption
                        SearchOption {
                            option: suggestionRow.modelData.option
                            lit: suggestionRow.modelData.option.id === root.view.activeId
                            onPicked: core.pick(option.id)
                        }
                    }
                    Component {
                        id: suggestionHeading
                        SearchHeading {
                            text: suggestionRow.modelData.heading
                            first: suggestionRow.modelData.first
                        }
                    }
                }
            }
        }
    }

    Flickable {
        id: body
        anchors.fill: parent
        topMargin: bar.y + bar.height + 8
        bottomMargin: root.bottomInset + 16
        onTopMarginChanged: contentY = -topMargin
        visible: !core.suggestionsShown
        contentHeight: bodyColumn.height
        boundsBehavior: Flickable.StopAtBounds
        // Reading the results puts the keyboard away.
        onMovementStarted: Qt.inputMethod.hide()
        onContentYChanged: core.maybeLoadMore(body)
        onContentHeightChanged: core.maybeLoadMore(body)
        onHeightChanged: core.maybeLoadMore(body)

        Column {
            id: bodyColumn
            x: root.leftInset
            width: body.width - root.leftInset - root.rightInset

            // The count and the sort.
            Item {
                width: parent.width
                height: 48
                visible: root.view.status === "success" && (root.view.total ?? 0) > 0
                Row {
                    x: 16
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4
                    Text {
                        anchors.baseline: resultWord.baseline
                        textFormat: Text.PlainText
                        text: Number(root.view.total ?? 0).toLocaleString(Qt.locale(), "f", 0)
                        color: Theme.ink2
                        font: Theme.type.num
                    }
                    Text {
                        id: resultWord
                        textFormat: Text.PlainText
                        text: root.view.total === 1 ? qsTr("result") : qsTr("results")
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                }
                Row {
                    anchors.right: parent.right
                    anchors.rightMargin: 12
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 2
                    Accessible.role: Accessible.Grouping
                    Accessible.name: qsTr("Sort results")
                    Repeater {
                        model: core.sorts
                        delegate: SearchSortKey {
                            required property var modelData
                            text: modelData.label
                            checked: root.view.sort === modelData.key
                            onClicked: Search.setSort(modelData.key)
                        }
                    }
                }
            }

            // Idle: search options and recent searches.
            Column {
                x: 8
                width: parent.width - 16
                visible: core.idle
                bottomPadding: 24
                Repeater {
                    model: core.idleRows
                    delegate: Loader {
                        id: idleRow
                        required property var modelData
                        width: parent ? parent.width : 0
                        sourceComponent: modelData.heading !== undefined ? idleHeading : idleOption
                        Component {
                            id: idleOption
                            SearchOption {
                                option: idleRow.modelData.option
                                onPicked: {
                                    core.pick(option.id)
                                    root.focusInput()
                                }
                            }
                        }
                        Component {
                            id: idleHeading
                            SearchHeading {
                                text: idleRow.modelData.heading
                                first: idleRow.modelData.first
                            }
                        }
                    }
                }
            }

            // Three static result rows in the row's own geometry.
            Column {
                x: 8
                width: parent.width - 16
                visible: root.view.status === "loading"
                topPadding: 12
                Accessible.role: Accessible.StaticText
                Accessible.name: qsTr("Searching")
                Item {
                    width: parent.width
                    height: 24
                    Rectangle { x: 8; anchors.verticalCenter: parent.verticalCenter; width: 80; height: 10; radius: Theme.radius; color: Theme.tint2 }
                }
                Repeater {
                    model: [0.82, 0.58, 0.7]
                    delegate: Item {
                        id: bone
                        required property real modelData
                        width: parent.width
                        height: 56
                        Rectangle { x: 8; y: 8; width: 24; height: 24; radius: Theme.radius; color: Theme.tint2 }
                        Rectangle { x: 42; y: 12; width: 112; height: 10; radius: Theme.radius; color: Theme.tint2 }
                        Rectangle { x: 42; y: 30; width: (bone.width - 50) * bone.modelData; height: 10; radius: Theme.radius; color: Theme.tint1 }
                    }
                }
            }

            SearchEmpty {
                width: parent.width
                visible: root.view.status === "error"
                title: qsTr("Search didn't load")
                message: qsTr("Check your connection and try again.")
                actionText: qsTr("Try again")
                actionIcon: "refresh"
                onAction: Search.retry()
            }

            SearchEmpty {
                width: parent.width
                visible: root.view.status === "success" && core.resultRows.length === 0
                title: qsTr("No messages found")
                message: root.view.emptyDescription ?? ""
                actionText: root.view.filtered === true ? qsTr("Clear search") : ""
                onAction: {
                    Search.clear()
                    root.focusInput()
                }
            }

            // The results, a channel heading over each run of one channel.
            Column {
                id: resultColumn
                x: 8
                width: parent.width - 16
                visible: root.view.status === "success" && core.resultRows.length > 0
                bottomPadding: 8
                Repeater {
                    model: core.resultRows
                    delegate: Loader {
                        id: resultRow
                        required property var modelData
                        width: resultColumn.width
                        sourceComponent: modelData.result !== undefined ? resultItem : channelHeading
                        Component {
                            id: resultItem
                            SearchResult {
                                result: resultRow.modelData.result
                                ground: Theme.ply0
                                onJumpRequested: core.jump(result)
                            }
                        }
                        Component {
                            id: channelHeading
                            Item {
                                height: resultRow.modelData.first ? 32 : 44
                                Accessible.role: Accessible.Heading
                                Accessible.name: qsTr("In %1").arg(resultRow.modelData.header.name)
                                Row {
                                    x: 8
                                    anchors.bottom: parent.bottom
                                    height: 32
                                    spacing: 6
                                    Icon {
                                        anchors.verticalCenter: parent.verticalCenter
                                        name: resultRow.modelData.header.icon
                                        size: Theme.iconXs
                                        color: Theme.ink3
                                    }
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: Math.min(implicitWidth, resultColumn.width - 40)
                                        textFormat: Text.PlainText
                                        text: resultRow.modelData.header.name
                                        elide: Text.ElideRight
                                        color: Theme.ink3
                                        font: Theme.type.captionStrong
                                    }
                                }
                            }
                        }
                    }
                }

                // Paging: loading, a failed page, more, or the end.
                Item {
                    width: parent.width
                    height: 56
                    Row {
                        anchors.centerIn: parent
                        visible: root.view.loadingMore === true
                        spacing: 8
                        Spinner {
                            anchors.verticalCenter: parent.verticalCenter
                            size: Theme.iconSm
                            running: root.view.loadingMore === true
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: qsTr("Loading more")
                            color: Theme.ink3
                            font: Theme.type.caption
                        }
                    }
                    KitButton {
                        anchors.centerIn: parent
                        visible: root.view.loadingMore !== true && root.view.loadMoreFailed === true
                        variant: "secondary"
                        size: "sm"
                        text: qsTr("Couldn't load more. Retry")
                        onClicked: Search.retry()
                    }
                    KitButton {
                        anchors.centerIn: parent
                        visible: root.view.loadingMore !== true && root.view.loadMoreFailed !== true && root.view.hasMore === true
                        variant: "ghost"
                        size: "sm"
                        text: qsTr("Load more")
                        onClicked: Search.loadMore()
                    }
                    Text {
                        anchors.centerIn: parent
                        visible: root.view.loadingMore !== true && root.view.loadMoreFailed !== true && root.view.hasMore !== true
                        text: qsTr("End of results")
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                }
            }
        }
    }
}
