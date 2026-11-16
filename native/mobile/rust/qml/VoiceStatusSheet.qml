pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// A voice channel's status, for someone connected there (the desktop's
// VoiceStatusDialog: VoiceChannelTextStatusModal.tsx and
// VoiceChannelMediaStatusModal.tsx), as one sheet with the two halves on a
// Status | Vibe switch. Status: the field (120 at most) with its count,
// then Clear and Set status. Vibe: the picture set now with Remove, the
// two sources (Upload: the system photo picker, one picture or clip up to
// the vibe limit; Choose GIF: the GIF pane, a tap sets it), then Recents,
// the channel's recent pictures as a masonry in which a tile sets that
// picture at once. A save that goes through closes the sheet; the server
// says why it refuses. The logic is Yap.Ui's VoiceStatusCore, shared with
// the desktop.
BottomSheet {
    id: sheet

    // text | media | gif
    property string view: "text"
    // The picture set now (the call's channel: we are connected there).
    readonly property var calls: {
        try {
            return JSON.parse(Calls.view)
        } catch (e) {
            return {}
        }
    }
    readonly property bool ours: Voice.channelId === core.channelId
    readonly property string currentMedia: ours ? (calls.channelStatusMedia ?? "") : ""
    VoiceStatusMedia {
        id: current
        media: sheet.currentMedia
    }
    // Remove is in flight until the picture goes.
    property bool removing: false
    onCurrentMediaChanged: removing = false
    // This sheet asked for the photo picker; the file it is uploading
    // (a picked copy, let go once the upload ends).
    property bool picking: false
    property string uploadPath: ""

    VoiceStatusCore {
        id: core
        draft: field.displayText
        onSaved: {
            Native.haptic(Native.Confirm)
            sheet.close()
        }
    }

    // `media` opens on the vibe.
    function openFor(channelId, name, media, text) {
        field.text = text
        core.start(channelId, name, media, text)
        sheet.view = media ? "media" : "text"
        sheet.removing = false
        // From the top: the vibe set now and Remove come first.
        mediaFlick.contentY = 0
        open()
    }
    function choose(which) {
        if (which === sheet.view)
            return
        Native.haptic(Native.Tick)
        Qt.inputMethod.hide()
        sheet.view = which
        if (which === "media")
            core.showMedia()
        else
            core.showText()
    }
    function save() {
        Qt.inputMethod.commit()
        if (!core.canSave)
            return
        Native.haptic(Native.Tick)
        core.save()
    }
    function pickFile() {
        if (core.mediaBusy || Native.pickingMedia)
            return
        Native.haptic(Native.Tick)
        core.localError = ""
        core.tooBig = ""
        sheet.picking = true
        Native.pickMedia(26214400)
    }
    function chooseGif() {
        if (core.mediaBusy)
            return
        Native.haptic(Native.Tick)
        sheet.view = "gif"
        gifPane.start("gifs")
    }

    Connections {
        target: Native
        function onMediaPicked(paths, failed) {
            if (!sheet.picking)
                return
            sheet.picking = false
            // One picture: the rest of a several-item pick goes.
            for (let i = 1; i < paths.length; ++i)
                Native.discardPicked(paths[i])
            if (paths.length === 0) {
                if (failed > 0)
                    core.localError = qsTr("That file couldn't be read, or it is over 25 MB. Try another one.")
                return
            }
            if (core.upload(paths[0]))
                sheet.uploadPath = paths[0]
            else
                Native.discardPicked(paths[0])
        }
    }
    readonly property bool uploading: core.status.uploading === true
    onUploadingChanged: {
        if (!uploading && uploadPath.length > 0) {
            Native.discardPicked(uploadPath)
            uploadPath = ""
        }
    }

    title: core.channelName
    contentHeight: view === "gif" ? 2000 : tabs.height + (view === "text" ? textBody.implicitHeight : mediaFlick.contentHeight)
    flickable: view === "gif" ? gifPane.flick : view === "media" ? mediaFlick : null
    onClosed: {
        gifPane.stop()
        core.close()
    }

    Item {
        id: tabs
        width: parent.width
        height: sheet.view === "gif" ? 48 : 64
        Segmented {
            visible: sheet.view !== "gif"
            x: 16
            y: 4
            width: parent.width - 32
            options: [qsTr("Status"), qsTr("Vibe")]
            current: sheet.view === "text" ? 0 : 1
            accessibleName: qsTr("Channel status")
            onPicked: index => sheet.choose(index === 0 ? "text" : "media")
        }
        // The GIF pane on its start screen: back to the vibe (in a list,
        // the pane's own back key steps out of it first).
        IconKey {
            visible: sheet.view === "gif" && gifPane.core.mode === "categories"
            x: 8
            anchors.verticalCenter: parent.verticalCenter
            iconName: "back"
            tone: "ghost"
            tip: qsTr("Back to the vibe")
            onClicked: {
                gifPane.stop()
                sheet.view = "media"
            }
        }
        Text {
            visible: sheet.view === "gif"
            x: gifPane.core.mode === "categories" ? 56 : 16
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: qsTr("Choose a GIF")
            color: Theme.ink1
            font: Theme.type.setting
            Accessible.role: Accessible.Heading
            Accessible.name: text
        }
    }

    // ---- Status -------------------------------------------------------------

    Column {
        id: textBody
        anchors.top: tabs.bottom
        visible: sheet.view === "text"
        x: 16
        width: parent.width - 32
        topPadding: 4
        bottomPadding: 8
        spacing: 8

        MobileField {
            id: field
            width: parent.width
            maximumLength: core.maxLength
            placeholderText: qsTr("Status for %1").arg(core.channelName)
            EnterKey.type: Qt.EnterKeyDone
            Accessible.name: qsTr("Channel status")
            onAccepted: sheet.save()
        }
        Item {
            width: parent.width
            height: Math.max(countText.implicitHeight, hintText.implicitHeight)
            Text {
                id: hintText
                width: parent.width - countText.width - 16
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                text: qsTr("Let others know what you're up to in the voice channel.")
                color: Theme.ink3
                font: Theme.type.caption
            }
            Text {
                id: countText
                anchors.right: parent.right
                textFormat: Text.PlainText
                text: qsTr("%1 / 120").arg(core.normalized.length)
                color: Theme.ink3
                font: Theme.type.num
            }
        }
        Text {
            visible: core.error.length > 0
            width: parent.width
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            text: core.error
            color: Theme.dangerInk
            font: Theme.type.caption
            Accessible.role: Accessible.AlertMessage
            Accessible.name: text
        }
        Item { width: 1; height: 4 }
        KitButton {
            width: parent.width
            size: "lg"
            variant: "primary"
            ground: Theme.ply3
            text: qsTr("Set status")
            loading: core.status.saving
            enabled: core.canSave
            onClicked: sheet.save()
        }
        KitButton {
            visible: field.displayText.length > 0
            width: parent.width
            size: "lg"
            variant: "secondary"
            text: qsTr("Clear")
            onClicked: {
                Native.haptic(Native.Tick)
                field.clear()
                field.forceActiveFocus()
            }
        }
    }

    // ---- Vibe ---------------------------------------------------------------

    Flickable {
        id: mediaFlick
        anchors.top: tabs.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        visible: sheet.view === "media"
        contentWidth: width
        contentHeight: mediaBody.implicitHeight
        boundsBehavior: Flickable.StopAtBounds
        clip: true

        Column {
            id: mediaBody
            x: 16
            width: mediaFlick.width - 32
            topPadding: 4
            bottomPadding: 16
            spacing: 12

            // What is set now, with Remove.
            Rectangle {
                id: nowPlate
                visible: current.info.url.length > 0
                width: parent.width
                height: visible ? Math.min(220, Math.round(width * current.info.h / current.info.w)) : 0
                radius: Theme.radius
                color: Theme.ply2
                border.width: 1
                border.color: Theme.line
                clip: true
                Accessible.role: Accessible.Graphic
                Accessible.name: qsTr("The channel's vibe")
                VibePicture {
                    anchors.fill: parent
                    anchors.margins: 1
                    url: current.info.url
                    video: current.info.video
                    playing: sheet.up && sheet.view === "media" && Qt.application.state === Qt.ApplicationActive
                }
            }
            KitButton {
                visible: nowPlate.visible
                width: parent.width
                size: "lg"
                variant: "secondary"
                iconName: "trash"
                text: qsTr("Remove vibe")
                loading: sheet.removing
                enabled: !core.mediaBusy && !sheet.removing
                onClicked: {
                    Native.haptic(Native.Tick)
                    sheet.removing = true
                    core.removeMedia()
                }
            }

            // The two sources, side by side.
            Row {
                id: sources
                width: parent.width
                spacing: 8
                readonly property real cell: (width - spacing) / 2
                Repeater {
                    model: [
                        { id: "upload", icon: "upload", text: qsTr("Upload"), hint: qsTr("A picture or a clip") },
                        { id: "gif", icon: "gif", text: qsTr("Choose GIF"), hint: qsTr("From the GIF picker") }
                    ]
                    Rectangle {
                        id: source
                        required property var modelData
                        readonly property bool busy: modelData.id === "upload" && (sheet.uploading || Native.pickingMedia)
                        width: sources.cell
                        height: 96
                        radius: Theme.radius
                        color: sourceTap.pressed ? Theme.tint3 : Theme.tint1
                        border.width: 1
                        border.color: Theme.line
                        opacity: core.mediaBusy && !busy ? 0.45 : 1
                        scale: sourceTap.pressed ? Theme.scalePressIcon : 1
                        Accessible.role: Accessible.Button
                        Accessible.name: source.modelData.text
                        Accessible.description: source.modelData.hint
                        Accessible.onPressAction: source.activate()
                        function activate() {
                            if (source.modelData.id === "upload")
                                sheet.pickFile()
                            else
                                sheet.chooseGif()
                        }
                        Column {
                            anchors.centerIn: parent
                            width: parent.width - 16
                            spacing: 6
                            Item {
                                anchors.horizontalCenter: parent.horizontalCenter
                                width: Theme.iconXl
                                height: Theme.iconXl
                                Icon {
                                    visible: !source.busy
                                    name: source.modelData.icon
                                    size: Theme.iconXl
                                    color: Theme.ink1
                                }
                                Spinner {
                                    anchors.centerIn: parent
                                    visible: source.busy
                                    running: visible
                                }
                            }
                            Text {
                                width: parent.width
                                horizontalAlignment: Text.AlignHCenter
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                                text: source.busy && sheet.uploading ? qsTr("Uploading…") : source.modelData.text
                                color: Theme.ink1
                                font: Theme.type.setting
                            }
                            Text {
                                width: parent.width
                                horizontalAlignment: Text.AlignHCenter
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                                text: source.modelData.hint
                                color: Theme.ink3
                                font: Theme.type.caption
                            }
                        }
                        TapHandler {
                            id: sourceTap
                            enabled: !core.mediaBusy
                            onTapped: source.activate()
                        }
                    }
                }
            }

            TooBigCard {
                visible: core.tooBig.length > 0
                width: parent.width
                info: core.tooBig.length > 0 ? JSON.parse(core.tooBig) : ({})
                onAction: sheet.pickFile()
            }
            Text {
                visible: core.error.length > 0
                width: parent.width
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                text: core.error
                color: Theme.dangerInk
                font: Theme.type.caption
                Accessible.role: Accessible.AlertMessage
                Accessible.name: text
            }

            // Recents, while they load or once there are some.
            Item {
                visible: core.status.mediaLoading || core.recents.length > 0
                width: parent.width
                height: 24
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: qsTr("Recents")
                    color: Theme.ink2
                    font: Theme.type.captionStrong
                    Accessible.role: Accessible.Heading
                    Accessible.name: text
                }
                Text {
                    visible: core.status.mediaLoading
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: qsTr("Loading…")
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
            Item {
                id: recents
                visible: core.recents.length > 0
                width: parent.width
                readonly property int cols: core.recentColumns(core.recents.length, false)
                readonly property var layout: core.masonry(core.recents, cols, width, 8, 10)
                height: layout.height

                Repeater {
                    model: core.recents
                    Rectangle {
                        id: recent
                        required property var modelData
                        required property int index
                        readonly property var box: recents.layout.tiles[index] ?? ({ x: 0, y: 0, w: 0, h: 0 })
                        x: box.x
                        y: box.y
                        width: box.w
                        height: box.h
                        radius: Theme.radius
                        color: recentTap.pressed ? Theme.tint2 : Theme.ply2
                        border.width: 1
                        border.color: Theme.line
                        opacity: core.mediaBusy ? 0.45 : 1
                        Accessible.role: Accessible.Button
                        Accessible.name: modelData.title.length > 0 ? modelData.title : qsTr("Recent voice channel status media")
                        Accessible.onPressAction: recent.use()
                        function use() {
                            if (core.mediaBusy)
                                return
                            Native.haptic(Native.Tick)
                            core.useRecent(recent.modelData.id)
                        }
                        Rectangle {
                            x: 5
                            y: 5
                            width: parent.width - 10
                            height: parent.height - 10
                            radius: Theme.radius
                            color: Theme.tint1
                            clip: true
                            VibePicture {
                                anchors.fill: parent
                                url: recent.modelData.url
                                video: recent.modelData.video
                                playing: sheet.up && sheet.view === "media" && Qt.application.state === Qt.ApplicationActive
                            }
                        }
                        TapHandler {
                            id: recentTap
                            onTapped: recent.use()
                        }
                    }
                }
            }
        }
    }

    // ---- GIF ----------------------------------------------------------------

    GifPane {
        id: gifPane
        anchors.top: tabs.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        visible: sheet.view === "gif"
        active: sheet.up && !sheet.closing && sheet.view === "gif"
        Component.onCompleted: gifPane.core.picking = true
        Connections {
            target: gifPane.core
            function onPicked(gifId) {
                Native.haptic(Native.Tick)
                core.useGif(gifId)
            }
            // The tile was taken: back to the vibe while it saves.
            function onDone() {
                gifPane.stop()
                sheet.view = "media"
            }
        }
    }
}
