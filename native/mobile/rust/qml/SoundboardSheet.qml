pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Templates as T
import Yap.Mobile
import Yap.Ui

// The soundboard in a call, as a sheet (SoundboardPicker.tsx at phone
// size, over the desktop picker's logic: Yap.Ui's SoundboardCore). From the
// top: the search (Listen Together and MyInstants beside it, not in a DM
// call), then what plays now (each with Stop), Favorites, each server's
// sounds (the call's server leads with Add sound when we may upload; in a
// DM call every member server), Custom sounds and Yap sounds, four cells a
// row. A tap plays the sound for everyone; a long press opens its sheet (Preview, the
// favourite, Edit, Delete; favourites live on the server). The foot holds
// Send (how loud our clips play for the room) and Soundboard (how loud
// clips play here, this phone's setting). Add sound picks an audio file
// (the files picker on audio/*) and opens the form: name, emoji, volume.
// Back steps out of MyInstants, the form or a delete before it closes the
// sheet.
Item {
    id: root

    // Over the page it is on, its header included.
    z: 100

    // soundboard | myinstants | form
    property string view: "soundboard"
    // upload | edit
    property string formMode: "upload"
    // The sound the form edits, the one a delete asks about, and the one
    // whose sheet is open.
    property var editing: null
    property var deleting: null
    property var acting: null
    // The picked file the form uploads (a copy in the app's cache).
    property string path: ""
    // This sheet asked for the picker (only it takes the result).
    property bool picking: false
    // A pick that failed, or a file that is not audio.
    property string notice: ""
    // The uploaded copy, dropped once the upload is over.
    property string sentPath: ""
    // Soundboard: this phone's setting, 0..100, saved a beat after the
    // last change (the desktop's scheduleVolumePersistence, 250 ms).
    property int boardVolume: 100

    readonly property bool up: board.up
    // The Listen Together key (not in a DM call): the host opens its sheet.
    signal listenTogether()
    // The shared lists: the soundboard filtered by the search, and the
    // MyInstants search.
    SoundboardCore {
        id: core
        query: search.displayText
        searchKind: "myinstants"
    }

    function open() {
        root.view = "soundboard"
        root.deleting = null
        root.acting = null
        root.notice = ""
        search.clear()
        flick.contentY = 0
        core.load()
        const saved = Apps.soundboardVolume()
        root.boardVolume = saved < 0 ? 100 : saved
        board.open()
    }
    function close() {
        board.close()
    }

    function showView(next) {
        Native.haptic(Native.Tick)
        Qt.inputMethod.hide()
        root.view = next
        root.deleting = null
        search.clear()
        if (next === "myinstants")
            core.search("myinstants", "")
    }
    function back() {
        Native.haptic(Native.Tick)
        if (root.deleting !== null)
            root.deleting = null
        else if (root.view === "form")
            root.closeForm()
        else
            root.showView("soundboard")
    }

    function play(sound) {
        Native.haptic(Native.Tick)
        core.play(sound)
    }
    function act(sound) {
        Native.haptic(Native.LongPress)
        root.acting = sound
        actions.open()
    }

    // Add sound: the files picker on audio, then the form.
    function startUpload() {
        Native.haptic(Native.Tick)
        root.notice = ""
        root.picking = true
        Native.pickAudio(50 * 1024 * 1024)
    }
    Connections {
        target: Native
        function onMediaPicked(paths, failed) {
            if (!root.picking)
                return
            root.picking = false
            if (paths.length > 0)
                root.startForm("upload", null, paths[0])
            else if (failed > 0)
                root.notice = qsTr("That file couldn't be read. Try another one.")
        }
    }
    function startForm(mode, sound, file) {
        core.clearError()
        root.formMode = mode
        root.editing = sound
        root.path = file ?? ""
        form.reset(sound ? sound.name : file.split("/").pop().replace(/\.[^.]+$/, ""),
                   sound ? sound.emoji : "",
                   sound ? Math.round((sound.volume ?? 1) * 100) : 100)
        root.view = "form"
        formFlick.contentY = 0
    }
    function closeForm() {
        Qt.inputMethod.hide()
        if (root.path.length > 0 && root.formMode === "upload")
            Native.discardPicked(root.path)
        root.path = ""
        root.editing = null
        root.view = "soundboard"
    }
    function saveForm(name, emoji, volume) {
        Native.haptic(Native.Confirm)
        Qt.inputMethod.hide()
        if (root.formMode === "edit") {
            Apps.editSound(root.editing.id, name, emoji, volume)
        } else {
            Apps.upload(root.path, name, emoji, volume)
            // The core reads the file as it uploads: the copy goes after.
            root.sentPath = root.path
        }
        root.path = ""
        root.editing = null
        root.view = "soundboard"
    }
    readonly property bool uploading: core.uploading
    onUploadingChanged: {
        if (!uploading && sentPath.length > 0) {
            Native.discardPicked(sentPath)
            sentPath = ""
        }
    }

    Timer {
        id: persist
        interval: 250
        onTriggered: Apps.setSoundboardVolume(root.boardVolume)
    }
    Timer {
        id: debounce
        // The web waits 400 ms after typing (none for the trending list).
        interval: 400
        onTriggered: core.search("myinstants", search.displayText)
    }

    // Off the board (MyInstants, the form, a delete), Back steps back to it
    // first (a back layer over the sheet's own).
    readonly property bool stepped: board.up && !board.closing
        && (root.view !== "soundboard" || root.deleting !== null)
    onSteppedChanged: stepped ? Native.pushBackLayer(root) : Native.popBackLayer(root)
    Component.onDestruction: Native.popBackLayer(root)
    function backProgressed(progress) {}
    function backCancelled() {}
    function backInvoked() {
        root.back()
    }

    readonly property real cellGap: 8
    readonly property real cellW: Math.floor((board.width - 32 - cellGap * 3) / 4)

    // ---- Pieces -------------------------------------------------------------

    // A section's label: 13/500 ink-3; a server's shows its icon first.
    component SectionLabel: Item {
        id: label
        property string text
        property string icon: ""
        property bool server: false
        width: parent ? parent.width : 0
        height: 40
        Row {
            x: 16
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - 32
            spacing: 8
            Rectangle {
                id: glyph
                visible: label.server
                anchors.verticalCenter: parent.verticalCenter
                width: 20
                height: 20
                radius: Theme.radius
                color: picture.status === Image.Ready ? "transparent" : Theme.tint3
                Text {
                    anchors.centerIn: parent
                    visible: picture.status !== Image.Ready
                    textFormat: Text.PlainText
                    text: label.text.slice(0, 1).toUpperCase()
                    color: Theme.ink1
                    font: Theme.type.silk
                }
                Image {
                    id: picture
                    anchors.fill: parent
                    visible: status === Image.Ready
                    source: label.icon
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                    sourceSize: Qt.size(40, 40)
                }
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - (label.server ? glyph.width + 8 : 0)
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: label.text
                color: Theme.ink3
                font: Theme.type.captionStrong
                Accessible.role: Accessible.Heading
            }
        }
    }

    // One sound: its emoji (or the soundboard glyph) over the name, two
    // lines at most; a lit star for a favourite, the talk LED while it
    // plays in the room. A tap plays it, a long press opens its sheet.
    component SoundCell: Rectangle {
        id: cell
        required property var sound
        width: root.cellW
        height: root.cellW
        radius: Theme.radius
        color: cellTap.pressed ? Theme.tint3 : Theme.tint1
        EaseColor on color {}
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Play %1").arg(sound.name)
        Accessible.description: sound.favorite ? qsTr("Favorite") : ""
        Accessible.onPressAction: root.play(cell.sound)

        Column {
            anchors.centerIn: parent
            width: parent.width - 8
            spacing: 4
            Item {
                anchors.horizontalCenter: parent.horizontalCenter
                width: 24
                height: 24
                Text {
                    id: emoji
                    anchors.centerIn: parent
                    visible: cell.sound.emoji.length > 0
                    textFormat: Text.PlainText
                    text: cell.sound.emoji
                    font.pixelSize: 20
                }
                Icon {
                    anchors.centerIn: parent
                    visible: !emoji.visible
                    name: cell.sound.kind === "myinstants" ? "zap" : "soundboard"
                    size: 24
                    color: Theme.ink3
                }
            }
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                textFormat: Text.PlainText
                text: cell.sound.name
                wrapMode: Text.WrapAnywhere
                maximumLineCount: 2
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.caption
            }
        }
        Icon {
            visible: cell.sound.favorite === true
            x: parent.width - width - 4
            y: 4
            name: "star"
            variant: "active"
            size: 14
            color: Theme.ink2
        }
        Led {
            visible: cell.sound.playing === true
            x: 6
            y: parent.height - height - 6
            lit: true
            color: Theme.talk
        }
        TapHandler {
            id: cellTap
            longPressThreshold: 0.4
            onTapped: root.play(cell.sound)
            onLongPressed: root.act(cell.sound)
        }
    }

    // The dashed-edge Add sound cell (a ruled edge on a phone).
    component AddCell: Rectangle {
        id: add
        width: root.cellW
        height: root.cellW
        radius: Theme.radius
        color: addTap.pressed ? Theme.tint2 : "transparent"
        border.width: 1
        border.color: Theme.lineStrong
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Add sound")
        Accessible.onPressAction: root.startUpload()
        Column {
            anchors.centerIn: parent
            spacing: 4
            Icon {
                anchors.horizontalCenter: parent.horizontalCenter
                name: "upload"
                size: 24
                color: Theme.ink2
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                textFormat: Text.PlainText
                text: qsTr("Add sound")
                color: Theme.ink2
                font: Theme.type.caption
            }
        }
        TapHandler {
            id: addTap
            onTapped: root.startUpload()
        }
    }

    component SoundGrid: Grid {
        id: grid
        property var sounds: []
        property bool canAdd: false
        x: 16
        columns: 4
        spacing: root.cellGap
        AddCell {
            visible: grid.canAdd
        }
        Repeater {
            model: grid.sounds
            delegate: SoundCell {
                required property var modelData
                sound: modelData
            }
        }
    }

    // A line of copy (empty sections, the search's states).
    component Note: Text {
        width: parent ? parent.width : 0
        leftPadding: 16
        rightPadding: 16
        topPadding: 4
        bottomPadding: 12
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        color: Theme.ink3
        font: Theme.type.description
    }

    // A volume at the foot: the glyph, the label, the range, the number.
    component FootVolume: Item {
        id: vol
        property string glyph
        property string label
        property string accessibleName
        property int value: 0
        signal moved(int value)
        height: 48
        Icon {
            id: volGlyph
            anchors.verticalCenter: parent.verticalCenter
            name: vol.glyph
            size: Theme.iconSm
            color: Theme.ink3
        }
        Text {
            id: volLabel
            anchors.left: volGlyph.right
            anchors.leftMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            width: 88
            elide: Text.ElideRight
            textFormat: Text.PlainText
            text: vol.label
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
            Accessible.name: vol.accessibleName
            Accessible.description: Math.round(value) + "%"
            onMoved: vol.moved(Math.round(value))
            Binding on value {
                when: !range.pressed
                value: vol.value
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
            text: Math.round(range.pressed ? range.value : vol.value)
            color: Theme.ink2
            font: Theme.type.num
        }
    }

    // ---- The sheet ----------------------------------------------------------

    BottomSheet {
        id: board
        anchors.fill: parent
        title: root.view === "form" ? (root.formMode === "edit" ? qsTr("Edit sound") : qsTr("Add sound"))
            : root.view === "myinstants" ? qsTr("MyInstants") : qsTr("Soundboard")
        // Long content: it rests at its detent and pulls up to full.
        contentHeight: 2000
        flickable: root.view === "form" ? formFlick : root.view === "myinstants" ? searchFlick : flick
        onClosed: {
            root.picking = false
            root.deleting = null
            if (root.view === "form")
                root.closeForm()
            root.view = "soundboard"
            if (persist.running) {
                persist.stop()
                Apps.setSoundboardVolume(root.boardVolume)
            }
        }

        // The search, with Back off the board and MyInstants on it.
        Item {
            id: head
            visible: root.view !== "form"
            width: parent.width
            height: visible ? Theme.controlLg + 8 : 0

            IconKey {
                id: backKey
                visible: root.view !== "soundboard"
                focusPolicy: Qt.NoFocus
                x: 4
                anchors.verticalCenter: plate.verticalCenter
                iconName: "back"
                tip: qsTr("Back")
                onClicked: root.back()
            }
            Rectangle {
                id: plate
                anchors.left: backKey.visible ? backKey.right : parent.left
                anchors.leftMargin: backKey.visible ? 4 : 16
                anchors.right: listenKey.visible ? listenKey.left : parent.right
                anchors.rightMargin: listenKey.visible ? 4 : 16
                height: Theme.controlLg
                radius: Theme.radius
                color: Theme.ply2
                border.width: 1
                border.color: search.activeFocus ? Theme.ink1 : Theme.lineField

                Icon {
                    id: glass
                    x: 14
                    anchors.verticalCenter: parent.verticalCenter
                    name: "search"
                    size: Theme.iconMd
                    color: Theme.ink3
                }
                TextField {
                    id: search
                    anchors.left: glass.right
                    anchors.leftMargin: 10
                    anchors.right: clearKey.visible ? clearKey.left : parent.right
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    background: null
                    padding: 0
                    color: Theme.ink1
                    placeholderText: root.view === "myinstants" ? qsTr("Search MyInstants") : qsTr("Search sounds")
                    placeholderTextColor: Theme.ink3
                    selectionColor: Theme.selection
                    selectedTextColor: Theme.ink1
                    font: Theme.type.message
                    inputMethodHints: Qt.ImhNoAutoUppercase
                    EnterKey.type: Qt.EnterKeySearch
                    Accessible.name: root.view === "myinstants" ? qsTr("Search MyInstants sounds") : qsTr("Search soundboard sounds")
                    onDisplayTextChanged: {
                        if (root.view === "myinstants")
                            debounce.restart()
                        else
                            flick.contentY = 0
                    }
                    onAccepted: Qt.inputMethod.hide()
                }
                IconKey {
                    id: clearKey
                    visible: search.displayText.length > 0
                    focusPolicy: Qt.NoFocus
                    anchors.right: parent.right
                    anchors.rightMargin: 4
                    anchors.verticalCenter: parent.verticalCenter
                    size: 32
                    tone: "muted"
                    iconName: "close"
                    tip: qsTr("Clear search")
                    onClicked: {
                        Qt.inputMethod.reset()
                        search.clear()
                    }
                }
            }
            IconKey {
                id: listenKey
                visible: root.view === "soundboard" && !core.dmCall
                focusPolicy: Qt.NoFocus
                anchors.right: instantsKey.left
                anchors.verticalCenter: plate.verticalCenter
                iconName: "headphones"
                tip: qsTr("Listen Together")
                Accessible.name: tip
                onClicked: {
                    Native.haptic(Native.Tick)
                    root.close()
                    root.listenTogether()
                }
            }
            IconKey {
                id: instantsKey
                visible: root.view === "soundboard" && !core.dmCall
                focusPolicy: Qt.NoFocus
                anchors.right: parent.right
                anchors.rightMargin: 8
                anchors.verticalCenter: plate.verticalCenter
                iconName: "zap"
                tip: qsTr("Search MyInstants")
                Accessible.name: tip
                onClicked: root.showView("myinstants")
            }
        }

        // What went wrong (the store's word, or a pick's).
        Rectangle {
            id: errorStrip
            readonly property string line: core.error.length > 0 ? core.error : root.notice
            anchors.top: head.bottom
            anchors.topMargin: visible ? 4 : 0
            x: 16
            width: parent.width - 32
            height: visible ? errorText.implicitHeight + 20 : 0
            visible: line.length > 0
            radius: Theme.radius
            color: Theme.dangerTint
            Accessible.role: Accessible.AlertMessage
            Accessible.name: line
            Text {
                id: errorText
                x: 12
                y: 10
                width: parent.width - 24 - dismissKey.width
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                text: errorStrip.line
                color: Theme.dangerInk
                font: Theme.type.description
            }
            IconKey {
                id: dismissKey
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                size: 40
                tone: "muted"
                iconName: "close"
                tip: qsTr("Dismiss")
                onClicked: {
                    core.clearError()
                    root.notice = ""
                }
            }
        }

        // ---- The board --------------------------------------------------
        Flickable {
            id: flick
            visible: root.view === "soundboard"
            anchors.top: errorStrip.bottom
            anchors.topMargin: 4
            anchors.bottom: deleteBox.visible ? deleteBox.top : foot.top
            width: parent.width
            contentHeight: boardBody.height + 8
            clip: true

            Column {
                id: boardBody
                width: flick.width

                // What plays in the room now, each with Stop.
                SectionLabel {
                    visible: core.playing.length > 0 && !core.searching
                    text: qsTr("Playing")
                }
                Repeater {
                    model: core.searching ? [] : core.playing
                    delegate: Item {
                        id: playRow
                        required property var modelData
                        width: boardBody.width
                        height: 56
                        Led {
                            id: playLed
                            x: 16
                            anchors.verticalCenter: parent.verticalCenter
                            lit: !playRow.modelData.paused
                            color: Theme.talk
                        }
                        Column {
                            anchors.left: playLed.right
                            anchors.leftMargin: 12
                            anchors.right: stopKey.left
                            anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            Text {
                                width: parent.width
                                elide: Text.ElideRight
                                textFormat: Text.PlainText
                                text: playRow.modelData.name
                                color: Theme.ink1
                                font: Theme.type.rowUnread
                            }
                            Text {
                                width: parent.width
                                elide: Text.ElideRight
                                textFormat: Text.PlainText
                                text: playRow.modelData.mine ? qsTr("You") : playRow.modelData.owner
                                color: Theme.ink3
                                font: Theme.type.caption
                            }
                        }
                        IconKey {
                            id: stopKey
                            anchors.right: parent.right
                            anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            size: 44
                            iconName: "stop"
                            tip: qsTr("Stop %1").arg(playRow.modelData.name)
                            Accessible.name: tip
                            onClicked: {
                                Native.haptic(Native.Tick)
                                Apps.stopPlayback(playRow.modelData.playbackId)
                            }
                        }
                    }
                }

                // Favorites (hidden while a search matches none).
                Column {
                    width: parent.width
                    visible: !core.searching || core.favorites.length > 0
                    SectionLabel { text: qsTr("Favorites") }
                    Note {
                        visible: core.favorites.length === 0
                        text: core.dmCall ? qsTr("No member-server favorites yet.")
                            : qsTr("No favorites yet. Long-press a sound to add it, or search MyInstants.")
                    }
                    SoundGrid {
                        visible: core.favorites.length > 0
                        sounds: core.favorites
                    }
                }

                // One section per server.
                Repeater {
                    model: core.sections
                    delegate: Column {
                        id: serverSection
                        required property var modelData
                        readonly property var sounds: core.sectionSounds(modelData)
                        readonly property bool canAdd: core.sectionCanAdd(modelData)
                        width: boardBody.width
                        visible: !core.searching || sounds.length > 0
                        SectionLabel {
                            server: true
                            text: serverSection.modelData.name
                            icon: serverSection.modelData.icon
                        }
                        Row {
                            visible: serverSection.sounds.length === 0 && !serverSection.canAdd && core.loading
                            leftPadding: 16
                            bottomPadding: 12
                            spacing: 8
                            Spinner {
                                anchors.verticalCenter: parent.verticalCenter
                                running: parent.visible && board.up
                            }
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                textFormat: Text.PlainText
                                text: qsTr("Loading sounds…")
                                color: Theme.ink3
                                font: Theme.type.caption
                            }
                        }
                        SoundGrid {
                            visible: serverSection.canAdd || serverSection.sounds.length > 0
                            canAdd: serverSection.canAdd
                            sounds: serverSection.sounds
                        }
                    }
                }

                // Custom sounds kept on this device.
                Column {
                    width: parent.width
                    visible: core.showCustom && (!core.searching || core.customs.length > 0)
                    SectionLabel { text: qsTr("Custom sounds") }
                    SoundGrid { sounds: core.customs }
                }

                // Yap sounds (the default tones).
                Column {
                    width: parent.width
                    visible: !core.searching || core.defaults.length > 0
                    SectionLabel { text: qsTr("Yap sounds") }
                    SoundGrid { sounds: core.defaults }
                }
            }
        }

        // Delete a sound, at the foot of the board.
        InlineConfirm {
            id: deleteBox
            visible: root.deleting !== null && root.view === "soundboard"
            x: 16
            width: parent.width - 32
            anchors.bottom: foot.top
            anchors.bottomMargin: 8
            consequence: core.deletePrompt(root.deleting)
            confirmLabel: qsTr("Delete")
            irreversible: root.deleting?.kind !== "custom"
            onCancelled: root.deleting = null
            onConfirmed: {
                core.remove(root.deleting)
                root.deleting = null
            }
        }

        // Send and Soundboard.
        Item {
            id: foot
            visible: root.view === "soundboard"
            anchors.bottom: parent.bottom
            width: parent.width
            height: visible ? footColumn.height + 8 : 0
            Rectangle {
                width: parent.width
                height: 1
                color: Theme.line
            }
            Column {
                id: footColumn
                x: 16
                y: 4
                width: parent.width - 32
                FootVolume {
                    width: parent.width
                    glyph: "voiceChannel"
                    label: qsTr("Send")
                    accessibleName: qsTr("Send volume: how loud your sounds play for everyone")
                    value: Math.round(core.sendVolume * 100)
                    onMoved: v => core.setSendVolume(v / 100)
                }
                FootVolume {
                    width: parent.width
                    glyph: "headphones"
                    label: qsTr("Soundboard")
                    accessibleName: qsTr("Soundboard volume: how loud sounds play on this phone")
                    value: root.boardVolume
                    onMoved: v => {
                        root.boardVolume = v
                        persist.restart()
                    }
                }
            }
        }

        // ---- MyInstants -------------------------------------------------
        Flickable {
            id: searchFlick
            visible: root.view === "myinstants"
            anchors.top: errorStrip.bottom
            anchors.topMargin: 4
            anchors.bottom: parent.bottom
            width: parent.width
            contentHeight: searchBody.height + 8
            clip: true
            readonly property var results: core.searchState.results ?? []
            readonly property bool busy: (core.searchState.searching === true || core.searchState.loaded !== true)
                && results.length === 0

            Column {
                id: searchBody
                width: searchFlick.width
                Item {
                    visible: searchFlick.busy
                    width: parent.width
                    height: 96
                    Spinner {
                        anchors.centerIn: parent
                        running: parent.visible && board.up
                    }
                    Accessible.role: Accessible.StaticText
                    Accessible.name: qsTr("Searching")
                }
                Note {
                    visible: !searchFlick.busy && searchFlick.results.length === 0
                    topPadding: 32
                    horizontalAlignment: Text.AlignHCenter
                    text: qsTr("No sounds found for \"%1\"").arg(core.searchState.query ?? "")
                }
                SectionLabel {
                    visible: searchFlick.results.length > 0 && (core.searchState.query ?? "").length === 0
                    text: qsTr("Trending")
                }
                SoundGrid {
                    sounds: root.view === "myinstants" ? searchFlick.results : []
                }
            }
        }

        // ---- Add or edit a sound ----------------------------------------
        Flickable {
            id: formFlick
            visible: root.view === "form"
            anchors.top: errorStrip.bottom
            anchors.bottom: parent.bottom
            width: parent.width
            contentHeight: form.height + 16
            clip: true

            SoundForm {
                id: form
                x: 16
                y: 4
                width: formFlick.width - 32
                mode: root.formMode
                path: root.path
                busy: core.uploading
                onSubmitted: (name, emoji, volume) => root.saveForm(name, emoji, volume)
                onCancelled: root.closeForm()
                onTooBigAction: which => {
                    root.closeForm()
                    if (which === "choose" || which === "pick")
                        root.startUpload()
                }
            }
        }
    }

    // A sound's own sheet (the desktop cell's chips).
    BottomSheet {
        id: actions
        anchors.fill: parent
        title: root.acting ? root.acting.name : ""
        contentHeight: actionColumn.implicitHeight
        onClosed: root.acting = null

        Column {
            id: actionColumn
            width: parent.width
            bottomPadding: 8
            SheetRow {
                text: qsTr("Play for everyone")
                iconName: "soundboard"
                onTriggered: {
                    const sound = root.acting
                    actions.close()
                    root.play(sound)
                }
            }
            SheetRow {
                visible: root.acting !== null && root.acting.kind !== "radio"
                text: qsTr("Preview (only you hear it)")
                iconName: "headphones"
                onTriggered: {
                    const sound = root.acting
                    actions.close()
                    Native.haptic(Native.Tick)
                    core.preview(sound)
                }
            }
            SheetRow {
                text: root.acting?.favorite === true ? qsTr("Remove from favorites") : qsTr("Add to favorites")
                iconName: "star"
                onTriggered: {
                    const sound = root.acting
                    actions.close()
                    Native.haptic(Native.Tick)
                    core.toggleFavorite(sound)
                }
            }
            SheetRow {
                visible: root.acting?.canEdit === true
                text: qsTr("Edit")
                iconName: "edit"
                onTriggered: {
                    const sound = root.acting
                    actions.close()
                    root.startForm("edit", sound, "")
                }
            }
            SheetRow {
                visible: root.acting?.canDelete === true
                text: qsTr("Delete")
                iconName: "trash"
                danger: true
                onTriggered: {
                    const sound = root.acting
                    actions.close()
                    root.view = "soundboard"
                    root.deleting = sound
                }
            }
        }
    }
}
