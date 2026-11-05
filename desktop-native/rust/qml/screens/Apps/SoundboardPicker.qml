pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// The soundboard (SoundboardPicker.tsx, soundboard view): search, the send
// volume, what is playing, the default tones and the server's sounds (a
// click plays for everyone), and Add sound for those who may upload.
AppPopup {
    id: root

    // The state is parsed afresh on every change, and a new object would
    // rebuild every Repeater under it; this slice is replaced only when its
    // content differs (as FriendsView and SearchPanel do).
    readonly property var parsed: JSON.parse(Apps.state || "{}")
    property var s: ({})
    property string sKey: "{}"

    function syncS() {
        const next = root.soundboardOf(parsed)
        const key = JSON.stringify(next)
        if (key !== sKey) {
            sKey = key
            s = next
        }
    }
    onParsedChanged: syncS()
    Component.onCompleted: syncS()

    // The soundboard's part of the state: Listen Together and Daily Word
    // changes leave it (and its Repeaters) alone.
    function soundboardOf(all) {
        const next = Object.assign({}, all)
        delete next.listen
        delete next.wordle
        delete next.others
        delete next.activity
        return next
    }
    readonly property string filter: search.text.trim().toLowerCase()
    property var deleting: null

    function shown(list) {
        return (list ?? []).filter(x => root.filter.length === 0 || x.name.toLowerCase().includes(root.filter))
    }

    title: qsTr("Soundboard")
    width: 420
    onOpened: {
        Apps.loadSounds()
        search.forceActiveFocus()
    }
    onClosed: deleting = null

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

            KitField {
                id: search
                width: parent.width
                small: true
                placeholderText: qsTr("Search sounds")
                Accessible.name: qsTr("Search sounds")
            }
            VolumeSlider {
                width: parent.width
                to: 100
                label: qsTr("Send volume")
                value: Math.round((root.s.sendVolume ?? 0.8) * 100)
                onMoved: Apps.setSendVolume(value / 100)
            }
            AppError {
                width: parent.width
                message: root.s.soundError ?? ""
                onDismissed: Apps.clearError()
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

            SectionLabel { text: qsTr("Yap sounds") }
            Flow {
                width: parent.width
                spacing: 6
                Repeater {
                    model: root.shown(root.s.defaults)
                    delegate: SoundTile {
                        required property var modelData
                        width: (column.width - 6) / 2
                        sound: modelData
                        onClicked: Apps.play(modelData.id)
                        onPreview: Apps.preview(modelData.id)
                    }
                }
            }

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
            Flow {
                width: parent.width
                spacing: 6
                Repeater {
                    model: root.shown(root.s.sounds)
                    delegate: SoundTile {
                        required property var modelData
                        width: (column.width - 6) / 2
                        sound: modelData
                        onClicked: Apps.play(modelData.id)
                        onPreview: Apps.preview(modelData.id)
                        onRemove: root.deleting = modelData
                    }
                }
            }
            Rectangle {
                visible: root.deleting !== null
                width: parent.width
                height: visible ? 48 : 0
                radius: Theme.radius
                color: Theme.dangerTint
                Text {
                    x: 12
                    width: parent.width - confirmRow.width - 24
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    text: qsTr("Delete %1? Everyone loses it.").arg(root.deleting?.name ?? "")
                    color: Theme.dangerInk
                    font: Theme.type.caption
                }
                Row {
                    id: confirmRow
                    anchors.right: parent.right
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 6
                    KitButton { size: "sm"; variant: "ghost"; text: qsTr("Cancel"); onClicked: root.deleting = null }
                    KitButton {
                        size: "sm"
                        variant: "danger"
                        text: qsTr("Delete")
                        onClicked: {
                            Apps.deleteSound(root.deleting.id)
                            root.deleting = null
                        }
                    }
                }
            }

            SoundUpload {
                visible: root.s.canUpload === true
                width: parent.width
                busy: root.s.uploading === true
            }
        }
    }

    component SectionLabel: Text {
        textFormat: Text.PlainText
        color: Theme.ink3
        font: Theme.type.captionStrong
    }
}
