pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// The soundboard (SoundboardPicker.tsx): the soundboard view (search, the
// send volume, what is playing, favourites, the server's sounds, custom
// sounds kept here, the default tones; a click plays for everyone) and the
// MyInstants and radio searches (a station goes to Listen Together).
AppPopup {
    id: root

    // The state is parsed afresh on every change, and a new object would
    // rebuild every Repeater under it; this slice is replaced only when its
    // content differs (as FriendsView and SearchPanel do).
    readonly property var parsed: JSON.parse(Apps.state || "{}")
    property var s: ({})
    property string sKey: "{}"
    // The MyInstants and radio searches, apart from the soundboard so that
    // typing (which changes only these) does not rebuild every grid's tiles.
    property var found: ({})
    property string foundKey: "{}"

    function syncS() {
        const next = root.soundboardOf(parsed)
        const key = JSON.stringify(next)
        if (key !== sKey) {
            sKey = key
            s = next
        }
        const nextFound = { instants: parsed.instants ?? {}, radio: parsed.radio ?? {} }
        const foundText = JSON.stringify(nextFound)
        if (foundText !== foundKey) {
            foundKey = foundText
            found = nextFound
        }
    }
    onParsedChanged: syncS()
    Component.onCompleted: syncS()

    // The soundboard's part of the state: Listen Together, Daily Word and
    // the searches leave it (and its Repeaters) alone.
    function soundboardOf(all) {
        const next = Object.assign({}, all)
        delete next.instants
        delete next.radio
        delete next.listen
        delete next.wordle
        delete next.others
        delete next.activity
        return next
    }

    // "soundboard", "myinstants" or "radio".
    property string view: "soundboard"
    readonly property string filter: search.text.trim().toLowerCase()
    readonly property var searchState: view === "radio" ? (found.radio ?? {}) : (found.instants ?? {})
    // A DM call has no server: no custom sounds there (as on the web).
    readonly property bool dmCall: s.inCall === true && (s.serverId ?? "") === ""
    property var deleting: null

    function shown(list) {
        return (list ?? []).filter(x => root.filter.length === 0 || x.name.toLowerCase().includes(root.filter))
    }
    function startEdit(sound) {
        editor.visible = true
        editor.start(sound)
    }
    function showView(next) {
        view = next
        search.text = ""
        if (next === "myinstants")
            Apps.searchInstants("")
        else if (next === "radio")
            Apps.searchRadio("")
        search.forceActiveFocus()
    }

    title: view === "myinstants" ? qsTr("MyInstants") : view === "radio" ? qsTr("Radio") : qsTr("Soundboard")
    width: 420
    onOpened: {
        view = "soundboard"
        Apps.loadSounds()
        Apps.loadFavorites()
        search.forceActiveFocus()
    }
    onClosed: {
        deleting = null
        editor.visible = false
    }

    // The web waits 400 ms after typing (none for the empty, trending list).
    Timer {
        id: debounce
        interval: 400
        onTriggered: {
            if (root.view === "myinstants")
                Apps.searchInstants(search.text)
            else if (root.view === "radio")
                Apps.searchRadio(search.text)
        }
    }

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
            spacing: 12

            Row {
                width: parent.width
                spacing: 4
                IconKey {
                    visible: root.view !== "soundboard"
                    anchors.verticalCenter: parent.verticalCenter
                    size: 32
                    iconName: "back"
                    tone: "muted"
                    tip: qsTr("Back")
                    onClicked: root.showView("soundboard")
                }
                KitField {
                    id: search
                    width: parent.width - (root.view === "soundboard" ? 72 : 36)
                    small: true
                    placeholderText: root.view === "myinstants" ? qsTr("Search MyInstants")
                        : root.view === "radio" ? qsTr("Search radio stations")
                        : qsTr("Search soundboard")
                    Accessible.name: placeholderText
                    onTextEdited: if (root.view !== "soundboard") debounce.restart()
                }
                IconKey {
                    visible: root.view === "soundboard"
                    anchors.verticalCenter: parent.verticalCenter
                    size: 32
                    iconName: "soundboard"
                    tone: "muted"
                    tip: qsTr("Search radio")
                    onClicked: root.showView("radio")
                }
                IconKey {
                    visible: root.view === "soundboard"
                    anchors.verticalCenter: parent.verticalCenter
                    size: 32
                    iconName: "zap"
                    tone: "muted"
                    tip: qsTr("Search MyInstants")
                    onClicked: root.showView("myinstants")
                }
            }
            AppError {
                width: parent.width
                message: root.s.soundError ?? ""
                onDismissed: Apps.clearError()
            }

            // MyInstants and radio results.
            Text {
                visible: root.view !== "soundboard" && (root.searchState.results ?? []).length === 0
                width: parent.width
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                text: root.searchState.searching || !root.searchState.loaded ? qsTr("Searching…")
                    : root.view === "radio" ? qsTr("No stations found for \"%1\"").arg(search.text)
                    : qsTr("No sounds found for \"%1\"").arg(search.text)
                color: Theme.ink3
                font: Theme.type.caption
            }
            SectionLabel {
                visible: root.view !== "soundboard" && (root.searchState.query ?? "").length === 0
                    && (root.searchState.results ?? []).length > 0
                text: root.view === "radio" ? qsTr("Top stations") : qsTr("Trending")
            }
            Flow {
                visible: root.view !== "soundboard"
                width: parent.width
                spacing: 6
                Repeater {
                    model: root.view !== "soundboard" ? (root.searchState.results ?? []) : []
                    delegate: SoundTile {
                        required property var modelData
                        width: (column.width - 6) / 2
                        sound: modelData
                        onClicked: Apps.play(modelData.id)
                        onPreview: Apps.preview(modelData.id)
                        onFavorite: Apps.toggleFavorite(modelData.id)
                    }
                }
            }

            // The soundboard.
            Column {
                visible: root.view === "soundboard"
                width: parent.width
                spacing: 12

                VolumeSlider {
                    width: parent.width
                    to: 100
                    label: qsTr("Send volume")
                    value: Math.round((root.s.sendVolume ?? 0.8) * 100)
                    onMoved: Apps.setSendVolume(value / 100)
                }
                SoundEdit {
                    id: editor
                    visible: false
                    width: parent.width
                    onDone: visible = false
                }

                SectionLabel { visible: (root.s.playing ?? []).length > 0; text: qsTr("Playing") }
                Repeater {
                    model: root.s.playing ?? []
                    delegate: PlaybackRow {
                        required property var modelData
                        width: column.width
                        playback: modelData
                    }
                }

                SectionLabel {
                    visible: root.filter.length === 0 || root.shown(root.s.favorites).length > 0
                    text: qsTr("Favorites")
                }
                Text {
                    visible: root.filter.length === 0 && (root.s.favorites ?? []).length === 0
                    width: parent.width
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    text: qsTr("No favorites yet. Search MyInstants to add some!")
                    color: Theme.ink3
                    font: Theme.type.caption
                }
                SoundGrid { width: parent.width; sounds: root.shown(root.s.favorites) }

                SectionLabel { text: qsTr("Server sounds") }
                Text {
                    visible: root.shown(root.s.sounds).length === 0
                    width: parent.width
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    text: root.s.loadingSounds ? qsTr("Loading sounds…")
                        : root.filter.length > 0 ? qsTr("No sounds match your search.")
                        : qsTr("No server sounds yet.")
                    color: Theme.ink3
                    font: Theme.type.caption
                }
                SoundGrid {
                    width: parent.width
                    sounds: root.shown(root.s.sounds)
                    onEditSound: sound => root.startEdit(sound)
                    onRemoveSound: sound => root.deleting = sound
                }
                DeleteConfirm {
                    width: parent.width
                    sound: root.deleting !== null && root.deleting.kind !== "custom" ? root.deleting : null
                    onCancelled: root.deleting = null
                    onConfirmed: {
                        Apps.deleteSound(root.deleting.id)
                        root.deleting = null
                    }
                }
                SoundUpload {
                    visible: root.s.canUpload === true && root.filter.length === 0
                    width: parent.width
                    busy: root.s.uploading === true
                }

                SectionLabel {
                    visible: !root.dmCall && (root.filter.length === 0 || root.shown(root.s.local).length > 0)
                    text: qsTr("Custom sounds")
                }
                SoundGrid {
                    visible: !root.dmCall
                    width: parent.width
                    sounds: root.dmCall ? [] : root.shown(root.s.local)
                    onEditSound: sound => root.startEdit(sound)
                    onRemoveSound: sound => root.deleting = sound
                }
                DeleteConfirm {
                    width: parent.width
                    sound: !root.dmCall && root.deleting !== null && root.deleting.kind === "custom" ? root.deleting : null
                    onCancelled: root.deleting = null
                    onConfirmed: {
                        Apps.deleteSound(root.deleting.id)
                        root.deleting = null
                    }
                }
                SoundUpload {
                    visible: !root.dmCall && root.s.inCall === true && root.filter.length === 0
                    width: parent.width
                    local: true
                }

                SectionLabel { text: qsTr("Yap sounds") }
                SoundGrid {
                    width: parent.width
                    sounds: root.shown(root.s.defaults)
                    onEditSound: sound => root.startEdit(sound)
                    onRemoveSound: sound => root.deleting = sound
                }
            }
        }
    }

    component SectionLabel: Text {
        textFormat: Text.PlainText
        color: Theme.ink3
        font: Theme.type.captionStrong
    }

    component SoundGrid: Flow {
        id: grid
        property var sounds: []
        signal editSound(var sound)
        signal removeSound(var sound)
        spacing: 6
        Repeater {
            model: grid.sounds
            delegate: SoundTile {
                required property var modelData
                width: (grid.width - 6) / 2
                sound: modelData
                onClicked: Apps.play(modelData.id)
                onPreview: Apps.preview(modelData.id)
                onFavorite: Apps.toggleFavorite(modelData.id)
                onRemove: grid.removeSound(modelData)
                onEdit: grid.editSound(modelData)
            }
        }
    }

    // Delete a sound (DeleteSoundDialog.tsx), under the section it is in.
    component DeleteConfirm: Rectangle {
        id: confirm
        property var sound: null
        signal cancelled()
        signal confirmed()
        visible: sound !== null
        height: visible ? 48 : 0
        radius: Theme.radius
        color: Theme.dangerTint
        Text {
            x: 12
            width: parent.width - confirmRow.width - 24
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            elide: Text.ElideRight
            text: confirm.sound?.kind === "custom"
                ? qsTr("Delete %1 from this device?").arg(confirm.sound?.name ?? "")
                : qsTr("Delete %1? Everyone loses it.").arg(confirm.sound?.name ?? "")
            color: Theme.dangerInk
            font: Theme.type.caption
        }
        Row {
            id: confirmRow
            anchors.right: parent.right
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 6
            KitButton { size: "sm"; variant: "ghost"; text: qsTr("Cancel"); onClicked: confirm.cancelled() }
            KitButton { size: "sm"; variant: "danger"; text: qsTr("Delete"); onClicked: confirm.confirmed() }
        }
    }
}
