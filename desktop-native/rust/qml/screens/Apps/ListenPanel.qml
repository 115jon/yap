pragma ComponentBehavior: Bound
import QtQuick
import Yap.Chrome
import com.yap.native

// Listen Together (ListenTogetherPanel.tsx): search tracks or collections,
// or paste a YouTube or Spotify link (a playlist previews with Play now and
// Queue); queue a track (add, play next, play now) or a whole collection;
// the queue (play, remove, Clear queue) and Recently played (Requeue); the
// room's volume on this device and the loudness preset.
AppPopup {
    id: root

    // The state is parsed afresh on every change, and a new object would
    // rebuild every Repeater under it; this slice is replaced only when its
    // content differs (as FriendsView and SearchPanel do).
    readonly property var parsed: JSON.parse(Apps.state || "{}")
    property var l: ({})
    property string lKey: "{}"

    function syncL() {
        const next = parsed.listen ?? {}
        const key = JSON.stringify(next)
        if (key !== lKey) {
            lKey = key
            l = next
        }
    }
    onParsedChanged: syncL()
    Component.onCompleted: syncL()
    property bool confirmClear: false
    // "queue" or "recent".
    property string tab: "queue"
    readonly property bool preview: (l.collectionTitle ?? "").length > 0
    readonly property bool collections: l.filter === "collection"

    function played(ms) {
        return Qt.formatTime(new Date(ms), Locale.ShortFormat)
    }

    title: qsTr("Listen Together")
    width: 460
    onOpened: search.forceActiveFocus()

    // Ctrl+L: the search field, its text selected (ListenTogetherPanel.tsx).
    Shortcut {
        enabled: root.visible
        sequence: "Ctrl+L"
        context: Qt.WindowShortcut
        onActivated: {
            search.forceActiveFocus(Qt.ShortcutFocusReason)
            search.selectAll()
        }
    }
    onClosed: confirmClear = false

    Flickable {
        anchors.fill: parent
        anchors.margins: 12
        anchors.topMargin: 0
        contentHeight: column.height
        clip: true
        boundsBehavior: Flickable.StopAtBounds

        Column {
            id: column
            width: parent.width
            spacing: 10

            KitField {
                id: search
                width: parent.width
                small: true
                placeholderText: qsTr("Search or paste YouTube / Spotify links")
                Accessible.name: qsTr("Search or paste a link")
                Keys.onReturnPressed: Apps.ltSearch(text)
                Keys.onEnterPressed: Apps.ltSearch(text)
            }
            Item {
                width: searchTabs.width
                height: 32
                Row {
                    id: searchTabs
                    height: 32
                    Accessible.role: Accessible.PageTabList
                    Accessible.name: qsTr("Search for")
                    AppTab { text: qsTr("Tracks"); on: !root.collections; onClicked: Apps.ltFilter("track") }
                    AppTab { text: qsTr("Collections"); on: root.collections; onClicked: Apps.ltFilter("collection") }
                }
                TabRule { row: searchTabs }
            }
            Rectangle {
                visible: (root.l.feedback ?? "").length > 0
                width: parent.width
                height: visible ? feedbackText.implicitHeight + 16 : 0
                radius: Theme.radius
                color: Theme.tint1
                Accessible.role: Accessible.StaticText
                Accessible.name: feedbackText.text
                Text {
                    id: feedbackText
                    x: 12
                    width: parent.width - 24
                    anchors.verticalCenter: parent.verticalCenter
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    text: root.l.feedback ?? ""
                    color: Theme.ink2
                    font: Theme.type.caption
                }
            }
            AppError {
                width: parent.width
                message: root.l.error ?? ""
                onDismissed: Apps.ltClearError()
            }
            AppError {
                width: parent.width
                message: root.l.playbackError ?? ""
                onDismissed: Apps.ltClearError()
            }
            Text {
                visible: root.l.searching === true || ((root.l.query ?? "").length > 0
                    && (root.l.results ?? []).length === 0 && (root.l.collections ?? []).length === 0)
                textFormat: Text.PlainText
                text: root.l.resolving ? qsTr("Resolving link preview…")
                    : root.l.searching ? qsTr("Searching…") : qsTr("No results")
                color: Theme.ink3
                font: Theme.type.caption
            }

            // A pasted playlist or album: one card, Play now or Queue it all.
            Rectangle {
                visible: root.preview
                width: parent.width
                height: visible ? 88 : 0
                radius: Theme.radius
                color: Theme.tint1
                TrackRow {
                    x: 4
                    y: 4
                    width: parent.width - 8
                    title: root.l.collectionTitle ?? ""
                    artist: root.l.collectionMeta ?? ""
                    artwork: root.l.collectionArtwork ?? ""
                }
                Row {
                    x: 56
                    y: 52
                    spacing: 8
                    KitButton { size: "sm"; variant: "primary"; iconName: "play"; text: qsTr("Play now"); onClicked: Apps.ltEnqueueAll("play-now") }
                    KitButton { size: "sm"; variant: "secondary"; text: qsTr("Queue"); onClicked: Apps.ltEnqueueAll("append") }
                }
            }
            Repeater {
                model: root.l.results ?? []
                delegate: TrackRow {
                    id: result
                    required property var modelData
                    width: column.width
                    title: modelData.title
                    artist: modelData.artist
                    artwork: modelData.artwork
                    durationMs: modelData.durationMs
                    IconKey { visible: !root.preview; size: 28; iconName: "queueAdd"; tone: "muted"; tip: qsTr("Queue"); onClicked: Apps.ltEnqueue(result.modelData.index, "append") }
                    IconKey { visible: !root.preview; size: 28; iconName: "playlist"; tone: "muted"; tip: qsTr("Play next"); onClicked: Apps.ltEnqueue(result.modelData.index, "play-next") }
                    IconKey { visible: !root.preview; size: 28; iconName: "play"; tone: "muted"; tip: qsTr("Play now"); onClicked: Apps.ltEnqueue(result.modelData.index, "play-now") }
                    IconKey {
                        id: trackMore
                        visible: !root.preview
                        size: 28
                        iconName: "more"
                        tone: "muted"
                        tip: qsTr("More actions for %1").arg(result.modelData.title)
                        onClicked: resultMenu.openFor(false, result.modelData, trackMore, 0, trackMore.height)
                    }
                    TapHandler {
                        acceptedButtons: Qt.RightButton
                        onTapped: point => resultMenu.openFor(false, result.modelData, result, point.position.x, point.position.y)
                    }
                }
            }
            Repeater {
                model: root.l.collections ?? []
                delegate: TrackRow {
                    id: collection
                    required property var modelData
                    width: column.width
                    title: modelData.title
                    artist: modelData.meta
                    artwork: modelData.artwork
                    tail: qsTr("Collection")
                    KitButton {
                        size: "sm"
                        variant: "secondary"
                        text: qsTr("Queue collection")
                        enabled: root.l.resolving !== true
                        onClicked: Apps.ltQueueCollection(collection.modelData.index)
                    }
                    IconKey {
                        id: collectionMore
                        size: 28
                        iconName: "more"
                        tone: "muted"
                        tip: qsTr("More actions for %1").arg(collection.modelData.title)
                        onClicked: resultMenu.openFor(true, collection.modelData, collectionMore, 0, collectionMore.height)
                    }
                    TapHandler {
                        acceptedButtons: Qt.RightButton
                        onTapped: point => resultMenu.openFor(true, collection.modelData, collection, point.position.x, point.position.y)
                    }
                }
            }
            Text {
                visible: (root.l.skipped ?? 0) > 0 && !root.preview
                textFormat: Text.PlainText
                text: qsTr("%1 tracks couldn't be matched and were skipped.").arg(root.l.skipped ?? 0)
                color: Theme.ink3
                font: Theme.type.caption
            }

            Item {
                width: parent.width
                height: 32
                Row {
                    id: activityTabs
                    height: parent.height
                    Accessible.role: Accessible.PageTabList
                    Accessible.name: qsTr("Listen Together room activity")
                    AppTab { text: qsTr("Queue"); count: (root.l.queue ?? []).length; on: root.tab === "queue"; onClicked: root.tab = "queue" }
                    AppTab { text: qsTr("Recently played"); count: (root.l.recent ?? []).length; on: root.tab === "recent"; onClicked: root.tab = "recent" }
                }
                TabRule { row: activityTabs }
                KitButton {
                    visible: root.tab === "queue" && (root.l.queue ?? []).length > 0 && !root.confirmClear
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    size: "sm"
                    variant: "ghost"
                    text: qsTr("Clear queue")
                    onClicked: root.confirmClear = true
                }
            }
            Row {
                visible: root.confirmClear
                height: visible ? implicitHeight : 0
                spacing: 8
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    width: column.width - 180
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    text: qsTr("Every queued track is removed for everyone listening in this room.")
                    color: Theme.ink1
                    font: Theme.type.caption
                }
                KitButton { size: "sm"; variant: "ghost"; text: qsTr("Cancel"); onClicked: root.confirmClear = false }
                KitButton {
                    size: "sm"
                    variant: "destructive"
                    text: qsTr("Clear")
                    onClicked: {
                        Apps.ltClear()
                        root.confirmClear = false
                    }
                }
            }
            Text {
                visible: root.tab === "queue" ? (root.l.queue ?? []).length === 0 : (root.l.recent ?? []).length === 0
                width: parent.width
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: root.tab === "queue" ? qsTr("Queue is empty. Search or paste a link to add the first track.")
                    : qsTr("Nothing played yet. Tracks show up here after they finish playing in this room.")
                color: Theme.ink3
                font: Theme.type.caption
            }
            Repeater {
                model: root.tab === "queue" ? (root.l.queue ?? []) : []
                delegate: TrackRow {
                    id: entry
                    required property var modelData
                    width: column.width
                    title: modelData.title
                    artist: [modelData.artist, modelData.requester.length > 0 ? qsTr("Added by %1").arg(modelData.requester) : "", modelData.batch]
                        .filter(x => x.length > 0).join(" · ")
                    artwork: modelData.artwork
                    durationMs: modelData.durationMs
                    tail: modelData.radio ? qsTr("Radio") : ""
                    current: modelData.current
                    IconKey { visible: !entry.modelData.current; size: 28; iconName: "play"; tone: "muted"; tip: qsTr("Play"); onClicked: Apps.ltPlay(entry.modelData.entryId) }
                    IconKey { size: 28; iconName: "trash"; tone: "muted"; tip: qsTr("Remove"); onClicked: Apps.ltRemove(entry.modelData.entryId) }
                }
            }
            Repeater {
                model: root.tab === "recent" ? (root.l.recent ?? []) : []
                delegate: TrackRow {
                    id: recent
                    required property var modelData
                    width: column.width
                    title: modelData.title
                    artist: modelData.meta
                    artwork: modelData.artwork
                    tail: root.played(modelData.playedAt)
                    KitButton { size: "sm"; variant: "secondary"; iconName: "queueAdd"; text: qsTr("Requeue"); onClicked: Apps.ltRequeue(recent.modelData.historyId) }
                }
            }

            VolumeSlider {
                width: parent.width
                to: 100
                label: qsTr("Listen Together volume")
                value: Math.round((root.l.volume ?? 1) * 100)
                onMoved: Apps.ltVolume(value / 100)
            }
            Row {
                spacing: 10
                SettingSwitch {
                    anchors.verticalCenter: parent.verticalCenter
                    checked: root.l.loudness !== false
                    Accessible.name: qsTr("Even out loudness")
                    onClicked: Apps.ltLoudness(!checked, root.l.preset ?? "balanced")
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: qsTr("Even out loudness")
                    color: Theme.ink2
                    font: Theme.type.caption
                }
                SelectField {
                    width: 150
                    enabled: root.l.loudness !== false
                    ids: ["peak-protection", "balanced", "night"]
                    names: [qsTr("Protect"), qsTr("Balanced"), qsTr("Night")]
                    current: root.l.preset ?? "balanced"
                    firstIsDefault: false
                    onPicked: id => Apps.ltLoudness(true, id)
                }
            }
        }
    }

    // A result's actions (ListenTogetherResultActions.tsx, the desktop
    // context menu): a track plays now, next or last, a collection queues
    // whole; both open their source page or copy its URL.
    MenuPanel {
        id: resultMenu
        property bool collection: false
        property var item: null
        function openFor(isCollection, item, anchor, x, y) {
            resultMenu.collection = isCollection
            resultMenu.item = item
            const at = anchor.mapToItem(root.contentItem, x, y)
            popup(root.contentItem, at.x, at.y)
        }
        MenuRow {
            text: qsTr("Play now")
            glyph: "play"
            visible: !resultMenu.collection
            height: visible ? implicitHeight : 0
            onTriggered: Apps.ltEnqueue(resultMenu.item.index, "play-now")
        }
        MenuRow {
            text: qsTr("Play next")
            glyph: "skip"
            visible: !resultMenu.collection
            height: visible ? implicitHeight : 0
            onTriggered: Apps.ltEnqueue(resultMenu.item.index, "play-next")
        }
        MenuRow {
            text: qsTr("Add to queue")
            glyph: "queueAdd"
            visible: !resultMenu.collection
            height: visible ? implicitHeight : 0
            onTriggered: Apps.ltEnqueue(resultMenu.item.index, "append")
        }
        MenuRow {
            text: qsTr("Add collection to queue")
            glyph: "queueAdd"
            visible: resultMenu.collection
            height: visible ? implicitHeight : 0
            enabled: root.l.resolving !== true
            onTriggered: Apps.ltQueueCollection(resultMenu.item.index)
        }
        MenuRow {
            text: qsTr("Open source URL")
            glyph: "externalLink"
            enabled: resultMenu.item !== null && resultMenu.item.url.length > 0
            onTriggered: Files.openExternal(resultMenu.item.url)
        }
        MenuRow {
            text: qsTr("Copy URL")
            glyph: "copy"
            enabled: resultMenu.item !== null && resultMenu.item.url.length > 0
            onTriggered: resultClipboard.copyText(resultMenu.item.url)
        }
    }
    ClipboardText { id: resultClipboard }
}
