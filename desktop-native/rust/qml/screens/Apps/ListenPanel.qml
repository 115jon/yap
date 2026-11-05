pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// Listen Together (ListenTogetherPanel.tsx): search YouTube or paste a
// link, queue a result (add, play next, play now), the queue with play and
// remove, Clear queue, the room's volume on this device and the loudness
// preset.
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

    title: qsTr("Listen Together")
    width: 460
    onOpened: search.forceActiveFocus()
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
                placeholderText: qsTr("Search YouTube or paste a link")
                Accessible.name: qsTr("Search YouTube or paste a link")
                Keys.onReturnPressed: Apps.ltSearch(text)
                Keys.onEnterPressed: Apps.ltSearch(text)
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
                visible: root.l.searching === true || ((root.l.query ?? "").length > 0 && (root.l.results ?? []).length === 0)
                textFormat: Text.PlainText
                text: root.l.searching ? qsTr("Searching…") : qsTr("No tracks found.")
                color: Theme.ink3
                font: Theme.type.caption
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
                    IconKey { size: 28; iconName: "queueAdd"; tone: "muted"; tip: qsTr("Add to queue"); onClicked: Apps.ltEnqueue(result.modelData.index, "append") }
                    IconKey { size: 28; iconName: "playlist"; tone: "muted"; tip: qsTr("Play next"); onClicked: Apps.ltEnqueue(result.modelData.index, "play-next") }
                    IconKey { size: 28; iconName: "play"; tone: "muted"; tip: qsTr("Play now"); onClicked: Apps.ltEnqueue(result.modelData.index, "play-now") }
                }
            }
            Text {
                visible: (root.l.skipped ?? 0) > 0
                textFormat: Text.PlainText
                text: qsTr("%1 tracks couldn't be matched and were skipped.").arg(root.l.skipped ?? 0)
                color: Theme.ink3
                font: Theme.type.caption
            }

            Item {
                width: parent.width
                height: 32
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: qsTr("Queue")
                    color: Theme.ink3
                    font: Theme.type.captionStrong
                }
                KitButton {
                    visible: (root.l.queue ?? []).length > 0 && !root.confirmClear
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
                visible: (root.l.queue ?? []).length === 0
                textFormat: Text.PlainText
                text: qsTr("Nothing queued. Search for a track to start listening together.")
                color: Theme.ink3
                font: Theme.type.caption
            }
            Repeater {
                model: root.l.queue ?? []
                delegate: TrackRow {
                    id: entry
                    required property var modelData
                    width: column.width
                    title: modelData.title
                    artist: modelData.requester.length > 0 ? qsTr("%1 · Added by %2").arg(modelData.artist).arg(modelData.requester) : modelData.artist
                    artwork: modelData.artwork
                    durationMs: modelData.durationMs
                    current: modelData.current
                    IconKey { visible: !entry.modelData.current; size: 28; iconName: "play"; tone: "muted"; tip: qsTr("Play"); onClicked: Apps.ltPlay(entry.modelData.entryId) }
                    IconKey { size: 28; iconName: "trash"; tone: "muted"; tip: qsTr("Remove"); onClicked: Apps.ltRemove(entry.modelData.entryId) }
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
}
