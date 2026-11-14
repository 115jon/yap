import QtQuick
import Yap.Ui

// What message search does, for both apps (SearchPanel.tsx). Not visual:
// the desktop's side panel and the phone's page each draw it their own way
// over the same rows. The store owns the rules (Search.state); this parses
// that state into the rows a list draws, rebuilding each part only when it
// changes, so typing does not recreate the result rows and the scroll
// position holds. `generation` moves with each new search (the list goes
// back to its top); `rewritten` hands the field text the store wrote (a
// chip, a pick, a clear).
QtObject {
    id: root

    // The parsed `Search.state`.
    property var view: ({})
    // Headings ({heading, first}) and options ({option}).
    property var suggestionRows: []
    property var idleRows: []
    // A server's channel headings ({header, first}) and results ({result}).
    property var resultRows: []
    property int generation: -1
    property int revision: -1

    property string suggestionKey
    property string idleKey
    property string resultKey

    readonly property string stateJson: Search.state
    onStateJsonChanged: parse()

    readonly property bool suggestionsShown: view.suggestionsOpen === true
    readonly property bool idle: view.status === "idle" || view.status === undefined
    readonly property bool canLoadMore: view.status === "success" && view.hasMore === true
        && view.loadMoreFailed !== true
    readonly property bool hasInput: (view.text ?? "").length > 0 || (view.chips ?? []).length > 0

    // The results toolbar's sorts.
    readonly property var sorts: [
        { key: "newest", label: qsTr("Newest") },
        { key: "oldest", label: qsTr("Oldest") },
        { key: "relevant", label: qsTr("Relevant") },
    ]

    // The field's text and caret, from the store.
    signal rewritten(string text, int caret)
    // A result was opened (its channel opens and the chat jumps to it).
    signal jumped(var result)

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
        if (next.generation !== undefined && next.generation !== root.generation)
            root.generation = next.generation
        if (next.revision !== undefined && next.revision !== root.revision) {
            root.revision = next.revision
            root.rewritten(next.text ?? "", next.caret ?? 0)
        }
    }

    // The search opens on the conversation in view (a server's, or a
    // DM's), with any typing handed over.
    function open(text) {
        parse()
        Search.open(text ?? "", Date.now())
    }
    function edit(text, caret) {
        Search.edit(text, caret, Date.now())
    }
    function pick(id) {
        Search.pick(id, Date.now())
    }
    function submit() {
        Search.submit(Date.now())
    }
    // Enter: the lit suggestion, else the search as typed.
    function enter() {
        const lit = root.suggestionsShown && (root.view.activeId ?? "").length > 0
        if (lit)
            root.pick(root.view.activeId)
        else
            root.submit()
    }

    function jump(result) {
        Search.opened(result.id)
        Chat.jumpToMessage(result.channelId, result.id)
        root.jumped(result)
    }

    // The next page, once a list scrolled near its end.
    function maybeLoadMore(flickable) {
        if (root.canLoadMore && !root.suggestionsShown
                && flickable.contentY + flickable.height >= flickable.contentHeight - 320)
            Search.loadMore()
    }

    Component.onCompleted: parse()
}
