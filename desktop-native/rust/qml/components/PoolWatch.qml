import QtQuick

// Tells an item inside a chat row when its list row is pooled or reused.
// ListView.onPooled/onReused reach only a delegate's root item, and a pooled
// row is culled, not hidden, so `visible` never says it. The row is the
// nearest ancestor with a `rowPooled` property (MessageDelegate sets it from
// its own ListView.onPooled and onReused); with none, nothing is signalled.
//
//   PoolWatch { owner: root; onPooled: stop(); onReused: reset() }
QtObject {
    id: watch

    required property Item owner
    property var row: null

    signal pooled()
    signal reused()

    Component.onCompleted: {
        for (let p = watch.owner.parent; p; p = p.parent) {
            if (p.rowPooled !== undefined) {
                watch.row = p
                break
            }
        }
    }

    property Connections link: Connections {
        target: watch.row
        ignoreUnknownSignals: true
        function onRowPooledChanged() {
            if (watch.row.rowPooled)
                watch.pooled()
            else
                watch.reused()
        }
    }
}
