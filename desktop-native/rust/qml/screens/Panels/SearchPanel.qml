pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// Message search (SearchPanel.tsx): the combobox under the header, then
// either the suggestions (typing a filter) or the body: the idle list
// (search options, recent searches), a static skeleton while loading, the
// failure and empty states, or the results grouped by channel with the
// count, the sort and paging. A result jumps to its message through
// `Chat.jumpToMessage`. The store owns the rules; this draws `Search.state`.
SidePanelFrame {
    id: root

    title: qsTr("Search")
    // SearchPanel.tsx: no title row; the field and a 28 px close key share
    // the 48 px header (px-3, gap 4).
    headerShown: false

    // The panel ply (search marks are solid over it): a ply-0 column.
    property color ground: Theme.ply0

    property var view: ({})
    // Rebuilt only when their part of the state changes, so typing does not
    // recreate the result rows (and the scroll position holds).
    property var suggestionRows: []
    property var idleRows: []
    property var resultRows: []
    property string suggestionKey
    property string idleKey
    property string resultKey
    property int generation: -1

    readonly property string stateJson: Search.state
    onStateJsonChanged: parse()

    readonly property bool suggestionsShown: view.suggestionsOpen === true
    readonly property bool canLoadMore: view.status === "success" && view.hasMore === true
        && view.loadMoreFailed !== true

    function flattenOptions(groups) {
        const rows = []
        for (const group of groups ?? []) {
            rows.push({ heading: group.heading, first: rows.length === 0 })
            for (const item of group.items)
                rows.push({ option: item })
        }
        return rows
    }

    function parse() {
        let next
        try {
            next = JSON.parse(root.stateJson)
        } catch (error) {
            next = {}
        }
        root.view = next
        const groups = JSON.stringify(next.groups ?? [])
        if (groups !== root.suggestionKey) {
            root.suggestionKey = groups
            root.suggestionRows = root.flattenOptions(next.groups)
        }
        const idle = JSON.stringify(next.idle ?? [])
        if (idle !== root.idleKey) {
            root.idleKey = idle
            root.idleRows = root.flattenOptions(next.idle)
        }
        const results = JSON.stringify([next.scope, next.results ?? []])
        if (results !== root.resultKey) {
            root.resultKey = results
            const rows = []
            for (const group of next.results ?? []) {
                if (next.scope === "server")
                    rows.push({ header: group, first: rows.length === 0 })
                for (const item of group.items)
                    rows.push({ result: item })
            }
            root.resultRows = rows
        }
        if (next.generation !== undefined && next.generation !== root.generation) {
            root.generation = next.generation
            body.contentY = 0
        }
    }

    function jump(result) {
        Search.opened(result.id)
        Chat.jumpToMessage(result.channelId, result.id)
    }

    function maybeLoadMore() {
        if (root.canLoadMore && !root.suggestionsShown && body.contentY + body.height >= body.contentHeight - 320)
            Search.loadMore()
    }

    // Arrow keys between results; up from the first goes back to the field.
    function stepResult(from, delta) {
        for (let i = from + delta; i >= 0 && i < resultRepeater.count; i += delta) {
            const loader = resultRepeater.itemAt(i)
            if (loader && loader.item && loader.isResult) {
                loader.item.forceActiveFocus()
                const top = loader.y + resultColumn.y
                if (top < body.contentY)
                    body.contentY = top
                else if (top + loader.height > body.contentY + body.height)
                    body.contentY = top + loader.height - body.height
                return
            }
        }
        if (delta < 0)
            field.focusInput()
    }

    Component.onCompleted: {
        parse()
        Search.open("", Date.now())
        field.focusInput()
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

    Item {
        id: fieldBar
        width: parent.width
        height: Theme.header
        SearchField {
            id: field
            anchors.left: parent.left
            anchors.right: closeKey.left
            anchors.leftMargin: 12
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            view: root.view
            onCloseRequested: root.closeRequested()
            onResultsRequested: root.stepResult(-1, 1)
        }
        IconKey {
            id: closeKey
            anchors.right: parent.right
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            iconName: "close"
            size: 28
            tone: "muted"
            tip: qsTr("Close search")
            tipSide: "bottom"
            onClicked: root.closeRequested()
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
    }

    // Inline suggestions replace the body while they are open.
    Flickable {
        id: suggestions
        anchors.top: fieldBar.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        visible: root.suggestionsShown
        contentHeight: suggestionColumn.height
        boundsBehavior: Flickable.StopAtBounds
        clip: true
        Accessible.role: Accessible.List
        Accessible.name: qsTr("Search suggestions")

        Column {
            id: suggestionColumn
            x: 8
            width: parent.width - 16
            topPadding: 4
            bottomPadding: 16
            Repeater {
                model: root.suggestionRows
                delegate: Loader {
                    id: suggestionRow
                    required property var modelData
                    width: suggestionColumn.width
                    sourceComponent: modelData.heading !== undefined ? groupHeading : suggestionOption
                    Component {
                        id: suggestionOption
                        SearchOption {
                            option: suggestionRow.modelData.option
                            lit: suggestionRow.modelData.option.id === root.view.activeId
                            onHovered: Search.hover(option.id)
                            onPicked: {
                                Search.pick(option.id, Date.now())
                                field.focusInput()
                            }
                            onLitChanged: {
                                if (!lit)
                                    return
                                const top = suggestionRow.y + suggestionColumn.y
                                if (top < suggestions.contentY)
                                    suggestions.contentY = top
                                else if (top + height > suggestions.contentY + suggestions.height)
                                    suggestions.contentY = top + height - suggestions.height
                            }
                        }
                    }
                    Component {
                        id: groupHeading
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
        anchors.top: fieldBar.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        visible: !root.suggestionsShown
        contentHeight: bodyColumn.height
        boundsBehavior: Flickable.StopAtBounds
        clip: true
        onContentYChanged: root.maybeLoadMore()
        onContentHeightChanged: root.maybeLoadMore()
        onHeightChanged: root.maybeLoadMore()

        ScrollBar.vertical: ThinScrollBar {}

        Column {
            id: bodyColumn
            width: body.width

            // The count and the sort (SearchPanel.tsx `toolbar`).
            Item {
                width: parent.width
                height: 40
                visible: root.view.status === "success" && (root.view.total ?? 0) > 0
                Row {
                    x: 16
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: 2
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
                    anchors.rightMargin: 16
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: 2
                    spacing: 2
                    Accessible.role: Accessible.Grouping
                    Accessible.name: qsTr("Sort results")
                    Repeater {
                        model: [
                            { key: "newest", label: qsTr("Newest") },
                            { key: "oldest", label: qsTr("Oldest") },
                            { key: "relevant", label: qsTr("Relevant") },
                        ]
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
                visible: root.view.status === "idle" || root.view.status === undefined
                topPadding: 4
                bottomPadding: 24
                Repeater {
                    model: root.idleRows
                    delegate: Loader {
                        id: idleRow
                        required property var modelData
                        width: parent ? parent.width : 0
                        sourceComponent: modelData.heading !== undefined ? idleHeading : idleOption
                        Component {
                            id: idleOption
                            SearchOption {
                                option: idleRow.modelData.option
                                activeFocusOnTab: true
                                onPicked: {
                                    Search.pick(option.id, Date.now())
                                    field.focusInput()
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
                visible: root.view.status === "success" && root.resultRows.length === 0
                title: qsTr("No messages found")
                message: root.view.emptyDescription ?? ""
                actionText: root.view.filtered === true ? qsTr("Clear search") : ""
                onAction: {
                    Search.clear()
                    field.focusInput()
                }
            }

            // The results, a channel header over each run of one channel.
            Column {
                id: resultColumn
                x: 8
                width: parent.width - 16
                visible: root.view.status === "success" && root.resultRows.length > 0
                bottomPadding: 24
                Repeater {
                    id: resultRepeater
                    model: root.resultRows
                    delegate: Loader {
                        id: resultRow
                        required property var modelData
                        required property int index
                        readonly property bool isResult: modelData.result !== undefined
                        width: resultColumn.width
                        sourceComponent: isResult ? resultItem : channelHeading
                        Component {
                            id: resultItem
                            SearchResult {
                                result: resultRow.modelData.result
                                ground: root.ground
                                onJumpRequested: root.jump(result)
                                Keys.onDownPressed: root.stepResult(resultRow.index, 1)
                                Keys.onUpPressed: root.stepResult(resultRow.index, -1)
                                Keys.onEscapePressed: field.focusInput()
                            }
                        }
                        Component {
                            id: channelHeading
                            Item {
                                height: resultRow.modelData.first ? 28 : 40
                                Accessible.role: Accessible.Heading
                                Accessible.name: qsTr("In %1").arg(resultRow.modelData.header.name)
                                Row {
                                    x: 8
                                    anchors.bottom: parent.bottom
                                    height: 28
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
                    height: 44
                    Row {
                        anchors.centerIn: parent
                        visible: root.view.loadingMore === true
                        spacing: 8
                        Icon {
                            id: moreSpinner
                            anchors.verticalCenter: parent.verticalCenter
                            name: "loader"
                            size: Theme.iconSm
                            color: Theme.ink3
                            smooth: true
                            RotationAnimator on rotation {
                                from: 0
                                to: 360
                                duration: Theme.durSpin
                                loops: Animation.Infinite
                                running: moreSpinner.visible && !Theme.reducedMotion
                            }
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
