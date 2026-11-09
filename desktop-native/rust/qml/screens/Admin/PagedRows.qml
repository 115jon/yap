import QtQuick

// A Repeater for a settings card's list that builds its rows a page at a
// time: the first `page`, then another page whenever `wanted` (the
// workbench scrolled near its end) holds. Its model is row numbers, so a
// list that changes (the admin state is one JSON) re-binds the rows in
// place instead of rebuilding them; delegates read their item as
// `list[index]` and must cope with `undefined` while the list shrinks.
Repeater {
    id: rows

    // How many items the list has.
    property int total: 0
    property int page: 20
    // The page's scroller is near its end: build more.
    property bool wanted: false
    property int shown: page

    readonly property bool hasMore: shown < total

    function more() {
        if (hasMore)
            shown += page
    }

    function sync() {
        const target = Math.min(total, shown)
        if (numbers.count > target)
            numbers.remove(target, numbers.count - target)
        while (numbers.count < target)
            numbers.append({ n: numbers.count })
        if (wanted && hasMore)
            settle.restart()
    }

    model: ListModel { id: numbers }

    onTotalChanged: sync()
    onShownChanged: sync()
    onWantedChanged: if (wanted) more()
    Component.onCompleted: sync()

    // Lets the new rows lay out (and the scroller's end move) before
    // asking again, so one scroll builds one page.
    property Timer settle: Timer {
        interval: 50
        onTriggered: if (rows.wanted) rows.more()
    }
}
