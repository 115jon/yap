pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Templates as T
import Yap.Mobile
import Yap.Ui

// Listen Together in a call (ListenTogetherPanel.tsx and the now-playing
// card at phone size, over the desktop's logic: Yap.Ui's ListenCore). Two
// faces:
// - A bar over the call's dock while the room plays something: the
//   artwork, the title, how it plays here (or that it is stopped on this
//   phone), and Listen or Stop. A tap opens the sheet.
// - The sheet: what plays (artwork, title, artist or Live Radio, who asked
//   for it, the clock for a track with a length), Listen or Stop here
//   (this phone only; the room plays on for everyone else), play or pause
//   and skip for the room, this phone's volume (saved for the next call:
//   a room already playing starts at it, half on a fresh install), then
//   Add (radio stations from radio-browser, or a track search or a
//   YouTube or Spotify link) and the queue (play, remove, Clear queue).
// The call page makes it; the soundboard's Listen Together key opens it.
Item {
    id: root

    // The bar shows while this is set (the call page: in the call).
    property bool shown: true
    // The dock's height under the bar.
    property real dockHeight: 0

    readonly property bool up: sheet.up

    ListenCore {
        id: core
        active: sheet.up
    }

    // The radio search (the soundboard's part of the state), replaced only
    // when it changes.
    property var radio: ({})
    property string radioKey: "{}"
    function syncRadio() {
        const next = core.parsed.radio ?? {}
        const key = JSON.stringify(next)
        if (key !== root.radioKey) {
            root.radioKey = key
            root.radio = next
        }
    }
    Connections {
        target: core
        function onParsedChanged() {
            root.syncRadio()
        }
    }
    // The soundboard's error line: a station that could not be queued.
    readonly property string radioError: core.parsed.soundError ?? ""

    // radio | tracks
    property string addKind: "radio"
    property bool confirmClear: false
    // This phone's volume, 0..100, as the slider shows it.
    readonly property int volumePercent: Math.round(core.volume * 100)

    function open() {
        root.confirmClear = false
        search.clear()
        flick.contentY = 0
        root.syncRadio()
        if (root.addKind === "radio" && root.radio.loaded !== true)
            Apps.searchRadio("")
        sheet.open()
    }
    function close() {
        sheet.close()
    }

    function setListening(on) {
        Native.haptic(Native.Tick)
        Apps.ltListen(on)
    }
    function showAdd(kind) {
        if (root.addKind === kind)
            return
        Native.haptic(Native.Tick)
        root.addKind = kind
        search.clear()
        if (kind === "radio" && root.radio.loaded !== true)
            Apps.searchRadio("")
    }
    function runSearch() {
        const text = search.displayText.trim()
        if (root.addKind === "radio")
            Apps.searchRadio(text)
        else if (text.length > 0)
            Apps.ltSearch(text)
    }
    // How it plays here, in a word.
    function stateWord() {
        if (!core.listening)
            return qsTr("Stopped on this phone")
        return core.paused ? qsTr("Paused") : qsTr("Playing")
    }

    // ---- Parts -----------------------------------------------------------

    // The artwork, or the music glyph on a tint.
    component Art: Rectangle {
        id: art
        property string source: ""
        property int size: 40
        width: size
        height: size
        radius: Theme.radius
        color: Theme.tint2
        Icon {
            anchors.centerIn: parent
            visible: picture.status !== Image.Ready
            name: "music"
            size: art.size >= 48 ? Theme.iconLg : Theme.iconMd
            color: Theme.ink3
        }
        Image {
            id: picture
            anchors.fill: parent
            visible: status === Image.Ready
            source: art.source
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            sourceSize: Qt.size(art.size * 2, art.size * 2)
        }
    }

    // A label over a group of rows.
    component Label: Text {
        width: parent ? parent.width : 0
        topPadding: 8
        bottomPadding: 4
        elide: Text.ElideRight
        textFormat: Text.PlainText
        color: Theme.ink3
        font: Theme.type.captionStrong
        Accessible.role: Accessible.Heading
        Accessible.name: text
    }

    // A line of copy (empty states, feedback).
    component Note: Text {
        width: parent ? parent.width : 0
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        color: Theme.ink3
        font: Theme.type.description
        Accessible.role: Accessible.StaticText
        Accessible.name: text
    }

    // A track or station row: the artwork, two lines, then its keys.
    component TrackLine: Item {
        id: line
        default property alias keys: keyRow.data
        property string title: ""
        property string meta: ""
        property string artwork: ""
        property bool current: false
        property bool showArt: true
        width: parent ? parent.width : 0
        height: 56
        Accessible.role: Accessible.ListItem
        Accessible.name: line.meta.length > 0 ? line.title + ", " + line.meta : line.title
        Art {
            id: lineArt
            visible: line.showArt
            anchors.verticalCenter: parent.verticalCenter
            source: line.artwork
        }
        Column {
            anchors.left: line.showArt ? lineArt.right : parent.left
            anchors.leftMargin: line.showArt ? 12 : 0
            anchors.right: keyRow.left
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Row {
                width: parent.width
                spacing: 6
                Led {
                    id: nowLed
                    visible: line.current
                    anchors.verticalCenter: parent.verticalCenter
                    lit: true
                    color: Theme.talk
                }
                Text {
                    width: parent.width - (nowLed.visible ? nowLed.width + 6 : 0)
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                    text: line.title
                    color: Theme.ink1
                    font: Theme.type.row
                }
            }
            Text {
                visible: line.meta.length > 0
                width: parent.width
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: line.meta
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
        Row {
            id: keyRow
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: 0
        }
    }

    // ---- The bar over the dock -------------------------------------------

    Rectangle {
        id: bar
        visible: root.shown && core.now !== null && !sheet.up
        anchors.bottom: parent.bottom
        anchors.bottomMargin: root.dockHeight + 8
        x: 12 + SafeArea.margins.left
        width: parent.width - 24 - SafeArea.margins.left - SafeArea.margins.right
        height: 56
        radius: Theme.radius
        color: barTap.pressed ? Theme.tint2 : Theme.ply3
        border.width: 1
        border.color: Theme.line
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Listen Together: %1, %2").arg(core.now?.title ?? "").arg(root.stateWord())
        Accessible.onPressAction: root.open()

        TapHandler {
            id: barTap
            onTapped: {
                Native.haptic(Native.Tick)
                root.open()
            }
        }
        Art {
            id: barArt
            x: 8
            anchors.verticalCenter: parent.verticalCenter
            source: core.now?.artwork ?? ""
        }
        Column {
            anchors.left: barArt.right
            anchors.leftMargin: 10
            anchors.right: barKey.left
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Text {
                width: parent.width
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: core.now?.title ?? ""
                color: Theme.ink1
                font: Theme.type.row
            }
            Text {
                width: parent.width
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: core.listening && !core.paused && core.nowArtist.length > 0
                    ? core.nowArtist : root.stateWord()
                color: core.listening ? Theme.ink3 : Theme.ink2
                font: Theme.type.caption
            }
        }
        IconKey {
            id: barKey
            anchors.right: parent.right
            anchors.rightMargin: 4
            anchors.verticalCenter: parent.verticalCenter
            size: 48
            iconName: core.listening ? "stop" : "headphones"
            tone: core.listening ? "ghost" : "toggled"
            tip: core.listening ? qsTr("Stop on this phone") : qsTr("Listen")
            Accessible.name: tip
            onClicked: root.setListening(!core.listening)
        }
    }

    // ---- The sheet -------------------------------------------------------

    BottomSheet {
        id: sheet
        anchors.fill: parent
        title: qsTr("Listen Together")
        // Long content: it rests at its detent and pulls up to full.
        contentHeight: body.height + 24
        flickable: flick
        onClosed: {
            root.confirmClear = false
            Qt.inputMethod.hide()
        }

        Flickable {
            id: flick
            anchors.fill: parent
            contentHeight: body.height + 24
            boundsBehavior: Flickable.StopAtBounds
            clip: true

            Column {
                id: body
                x: 16
                width: flick.width - 32
                spacing: 8

                // ── Now playing ──────────────────────────────────────────
                Note {
                    visible: core.now === null
                    topPadding: 4
                    text: qsTr("Nothing is playing in this call. Add a radio station or a track below and everyone here hears it.")
                }
                Item {
                    visible: core.now !== null
                    width: parent.width
                    height: visible ? 72 : 0
                    Art {
                        id: nowArt
                        size: 64
                        anchors.verticalCenter: parent.verticalCenter
                        source: core.now?.artwork ?? ""
                    }
                    Column {
                        anchors.left: nowArt.right
                        anchors.leftMargin: 12
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 3
                        Text {
                            width: parent.width
                            elide: Text.ElideRight
                            maximumLineCount: 2
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            text: core.now?.title ?? ""
                            color: Theme.ink1
                            font: Theme.type.setting
                            Accessible.role: Accessible.Heading
                            Accessible.name: text
                        }
                        Text {
                            visible: text.length > 0
                            width: parent.width
                            elide: Text.ElideRight
                            textFormat: Text.PlainText
                            text: [core.nowArtist, (core.now?.requester ?? "").length > 0
                                ? qsTr("Requested by %1").arg(core.now.requester) : ""]
                                .filter(x => x.length > 0).join(" · ")
                            color: Theme.ink3
                            font: Theme.type.caption
                        }
                        Row {
                            spacing: 6
                            Led {
                                anchors.verticalCenter: parent.verticalCenter
                                lit: core.listening && !core.paused
                                color: core.listening && !core.paused ? Theme.talk : Theme.lineStrong
                            }
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                textFormat: Text.PlainText
                                text: root.stateWord()
                                color: Theme.ink2
                                font: Theme.type.silk
                            }
                        }
                    }
                }
                // The clock, for a track with a length.
                Item {
                    visible: core.now !== null && core.durationMs > 0 && core.now?.radio !== true
                    width: parent.width
                    height: visible ? 28 : 0
                    Accessible.role: Accessible.ProgressBar
                    Accessible.name: qsTr("%1 of %2").arg(core.clock(core.position)).arg(core.clock(core.durationMs))
                    Text {
                        id: atText
                        anchors.left: parent.left
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: core.clock(core.position)
                        color: Theme.ink3
                        font: Theme.type.num
                    }
                    Rectangle {
                        anchors.left: atText.right
                        anchors.leftMargin: 10
                        anchors.right: lengthText.left
                        anchors.rightMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        height: 4
                        radius: 1
                        color: Theme.tint3
                        Rectangle {
                            width: parent.width * Math.max(0, Math.min(1, core.position / Math.max(1, core.durationMs)))
                            height: parent.height
                            radius: 1
                            color: Theme.accentInk
                        }
                    }
                    Text {
                        id: lengthText
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: core.clock(core.durationMs)
                        color: Theme.ink3
                        font: Theme.type.num
                    }
                }
                Text {
                    visible: (core.l.playbackError ?? "").length > 0
                    width: parent.width
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    text: core.l.playbackError ?? ""
                    color: Theme.dangerInk
                    font: Theme.type.description
                    Accessible.role: Accessible.AlertMessage
                    Accessible.name: text
                }
                // Listen or Stop here; play, pause and skip for the room.
                Item {
                    visible: core.now !== null
                    width: parent.width
                    height: visible ? Theme.controlLg : 0
                    KitButton {
                        anchors.left: parent.left
                        anchors.right: roomKeys.left
                        anchors.rightMargin: 8
                        size: "lg"
                        variant: core.listening ? "secondary" : "primary"
                        iconName: core.listening ? "stop" : "headphones"
                        text: core.listening ? qsTr("Stop on this phone") : qsTr("Listen")
                        Accessible.description: core.listening
                            ? qsTr("The room keeps playing for everyone else.") : ""
                        onClicked: root.setListening(!core.listening)
                    }
                    Row {
                        id: roomKeys
                        anchors.right: parent.right
                        spacing: 4
                        IconKey {
                            size: 48
                            iconName: core.paused ? "play" : "pause"
                            tone: "rest"
                            tip: core.paused ? qsTr("Play for everyone") : qsTr("Pause for everyone")
                            Accessible.name: tip
                            onClicked: {
                                Native.haptic(Native.Tick)
                                Apps.ltPause(!core.paused)
                            }
                        }
                        IconKey {
                            size: 48
                            iconName: "skip"
                            tone: "rest"
                            tip: qsTr("Skip for everyone")
                            Accessible.name: tip
                            onClicked: {
                                Native.haptic(Native.Tick)
                                Apps.ltSkip()
                            }
                        }
                    }
                }
                // This phone's volume (saved for the next call).
                Item {
                    width: parent.width
                    height: 48
                    Icon {
                        id: volGlyph
                        anchors.verticalCenter: parent.verticalCenter
                        name: root.volumePercent === 0 ? "speakerOff" : root.volumePercent < 50 ? "speakerLow" : "speakerHigh"
                        size: Theme.iconSm
                        color: Theme.ink3
                    }
                    Text {
                        id: volLabel
                        anchors.left: volGlyph.right
                        anchors.leftMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        width: 96
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                        text: qsTr("This phone")
                        color: Theme.ink2
                        font: Theme.type.captionStrong
                    }
                    T.Slider {
                        id: range
                        anchors.left: volLabel.right
                        anchors.leftMargin: 8
                        anchors.right: volNum.left
                        anchors.rightMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        height: 48
                        from: 0
                        to: 100
                        stepSize: 1
                        snapMode: T.Slider.SnapAlways
                        Accessible.role: Accessible.Slider
                        Accessible.name: qsTr("Listen Together volume on this phone")
                        Accessible.description: Math.round(value) + "%"
                        onMoved: Apps.ltVolume(Math.round(value) / 100)
                        Binding on value {
                            when: !range.pressed
                            value: root.volumePercent
                            restoreMode: Binding.RestoreNone
                        }
                        background: Rectangle {
                            x: range.leftPadding
                            y: (range.height - height) / 2
                            width: range.availableWidth
                            height: 6
                            radius: 1
                            color: Theme.tint3
                            Rectangle {
                                width: range.visualPosition * parent.width
                                height: parent.height
                                radius: 1
                                color: Theme.accentInk
                            }
                        }
                        handle: Rectangle {
                            x: range.leftPadding + range.visualPosition * (range.availableWidth - width)
                            y: (range.height - height) / 2
                            width: 14
                            height: 30
                            radius: Theme.radius
                            color: Theme.ink1
                        }
                    }
                    Text {
                        id: volNum
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        width: 32
                        horizontalAlignment: Text.AlignRight
                        textFormat: Text.PlainText
                        text: Math.round(range.pressed ? range.value : root.volumePercent)
                        color: Theme.ink2
                        font: Theme.type.num
                    }
                }

                // ── Add ──────────────────────────────────────────────────
                Label { text: qsTr("Add to the queue") }
                Row {
                    spacing: 8
                    Accessible.role: Accessible.PageTabList
                    Accessible.name: qsTr("Add from")
                    KitButton {
                        size: "sm"
                        variant: root.addKind === "radio" ? "primary" : "ghost"
                        iconName: "globe"
                        text: qsTr("Radio")
                        Accessible.role: Accessible.PageTab
                        Accessible.checked: root.addKind === "radio"
                        onClicked: root.showAdd("radio")
                    }
                    KitButton {
                        size: "sm"
                        variant: root.addKind === "tracks" ? "primary" : "ghost"
                        iconName: "music"
                        text: qsTr("Tracks")
                        Accessible.role: Accessible.PageTab
                        Accessible.checked: root.addKind === "tracks"
                        onClicked: root.showAdd("tracks")
                    }
                }
                MobileField {
                    id: search
                    width: parent.width
                    placeholderText: root.addKind === "radio" ? qsTr("Search radio stations")
                        : qsTr("Search or paste a YouTube or Spotify link")
                    Accessible.name: placeholderText
                    inputMethodHints: Qt.ImhNoAutoUppercase
                    EnterKey.type: Qt.EnterKeySearch
                    onDisplayTextChanged: {
                        if (root.addKind === "radio")
                            debounce.restart()
                    }
                    onAccepted: {
                        debounce.stop()
                        Qt.inputMethod.hide()
                        root.runSearch()
                    }
                }
                Timer {
                    id: debounce
                    interval: 350
                    onTriggered: root.runSearch()
                }

                // Radio: the top stations, or a search. A tap queues one.
                Note {
                    visible: root.addKind === "radio" && root.radioError.length > 0
                    text: root.radioError
                    color: Theme.dangerInk
                }
                Note {
                    visible: root.addKind === "radio" && root.radio.searching === true
                        && (root.radio.results ?? []).length === 0
                    text: qsTr("Searching…")
                }
                Note {
                    visible: root.addKind === "radio" && root.radio.loaded === true && root.radio.searching !== true
                        && (root.radio.results ?? []).length === 0
                    text: qsTr("No stations found for \"%1\"").arg(root.radio.query ?? "")
                }
                Repeater {
                    model: root.addKind === "radio" ? (root.radio.results ?? []) : []
                    delegate: TrackLine {
                        id: station
                        required property var modelData
                        title: modelData.name
                        meta: qsTr("Live Radio")
                        showArt: false
                        IconKey {
                            size: 48
                            iconName: "queueAdd"
                            tone: "ghost"
                            tip: qsTr("Queue %1").arg(station.modelData.name)
                            Accessible.name: tip
                            onClicked: {
                                Native.haptic(Native.Confirm)
                                Apps.play(station.modelData.id)
                            }
                        }
                    }
                }

                // Tracks: a search, or a pasted link (a playlist previews).
                Note {
                    visible: root.addKind === "tracks" && (core.l.feedback ?? "").length > 0
                    text: core.l.feedback ?? ""
                    color: Theme.ink2
                }
                Text {
                    visible: root.addKind === "tracks" && (core.l.error ?? "").length > 0
                    width: parent.width
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    text: core.l.error ?? ""
                    color: Theme.dangerInk
                    font: Theme.type.description
                    Accessible.role: Accessible.AlertMessage
                    Accessible.name: text
                }
                Note {
                    visible: root.addKind === "tracks" && core.searchShows
                    text: core.searchStatus
                }
                Item {
                    visible: root.addKind === "tracks" && core.preview
                    width: parent.width
                    height: visible ? 56 + 8 + Theme.controlLg : 0
                    TrackLine {
                        id: previewLine
                        title: core.l.collectionTitle ?? ""
                        meta: core.l.collectionMeta ?? ""
                        artwork: core.l.collectionArtwork ?? ""
                    }
                    Row {
                        anchors.top: previewLine.bottom
                        anchors.topMargin: 8
                        spacing: 8
                        KitButton {
                            size: "md"
                            variant: "primary"
                            iconName: "play"
                            text: qsTr("Play now")
                            onClicked: Apps.ltEnqueueAll("play-now")
                        }
                        KitButton {
                            size: "md"
                            variant: "secondary"
                            iconName: "queueAdd"
                            text: qsTr("Queue")
                            onClicked: Apps.ltEnqueueAll("append")
                        }
                    }
                }
                Repeater {
                    model: root.addKind === "tracks" && !core.preview ? (core.l.results ?? []) : []
                    delegate: TrackLine {
                        id: result
                        required property var modelData
                        title: modelData.title
                        meta: modelData.durationMs > 0
                            ? [modelData.artist, core.clock(modelData.durationMs)].filter(x => x.length > 0).join(" · ")
                            : modelData.artist
                        artwork: modelData.artwork
                        IconKey {
                            size: 48
                            iconName: "queueAdd"
                            tone: "ghost"
                            tip: qsTr("Queue %1").arg(result.modelData.title)
                            Accessible.name: tip
                            onClicked: {
                                Native.haptic(Native.Confirm)
                                Apps.ltEnqueue(result.modelData.index, "append")
                            }
                        }
                        IconKey {
                            size: 48
                            iconName: "play"
                            tone: "ghost"
                            tip: qsTr("Play %1 now").arg(result.modelData.title)
                            Accessible.name: tip
                            onClicked: {
                                Native.haptic(Native.Confirm)
                                Apps.ltEnqueue(result.modelData.index, "play-now")
                            }
                        }
                    }
                }

                // ── The queue ────────────────────────────────────────────
                Item {
                    width: parent.width
                    height: 40
                    Label {
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - clearKey.width - 8
                        text: core.queueCount > 0 ? qsTr("Queue · %1").arg(core.queueCount) : qsTr("Queue")
                    }
                    KitButton {
                        id: clearKey
                        visible: core.queueCount > 0 && !root.confirmClear
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        size: "sm"
                        variant: "ghost"
                        text: qsTr("Clear queue")
                        onClicked: root.confirmClear = true
                    }
                }
                InlineConfirm {
                    visible: root.confirmClear
                    height: visible ? implicitHeight : 0
                    consequence: qsTr("Every queued track is removed for everyone listening in this call.")
                    confirmLabel: qsTr("Clear")
                    onCancelled: root.confirmClear = false
                    onConfirmed: {
                        Apps.ltClear()
                        root.confirmClear = false
                    }
                }
                Note {
                    visible: core.queueCount === 0
                    text: qsTr("The queue is empty.")
                }
                Repeater {
                    model: core.queue
                    delegate: TrackLine {
                        id: entry
                        required property var modelData
                        title: modelData.title
                        meta: modelData.radio ? [qsTr("Live Radio"), core.queueMeta(modelData)].filter(x => x.length > 0).join(" · ")
                            : core.queueMeta(modelData)
                        artwork: modelData.artwork
                        current: modelData.current
                        IconKey {
                            visible: !entry.modelData.current
                            size: 48
                            iconName: "play"
                            tone: "ghost"
                            tip: qsTr("Play %1").arg(entry.modelData.title)
                            Accessible.name: tip
                            onClicked: {
                                Native.haptic(Native.Tick)
                                Apps.ltPlay(entry.modelData.entryId)
                            }
                        }
                        IconKey {
                            size: 48
                            iconName: "trash"
                            tone: "destructive"
                            tip: qsTr("Remove %1").arg(entry.modelData.title)
                            Accessible.name: tip
                            onClicked: {
                                Native.haptic(Native.Tick)
                                Apps.ltRemove(entry.modelData.entryId)
                            }
                        }
                    }
                }
            }
        }
    }
}
