import QtQuick
import Yap.Chrome
import com.yap.native

// The call strip above the account row while in a call (VoiceDashboard
// pared down): a 2 px ink-1 rule, the LED label and where the call is,
// then the action row (VoiceDashboard.tsx): camera, share, Activities,
// Soundboard and Leave, the one solid danger fill in a call surface. The
// row steps down so Leave is always whole: 48 px keys from 248 px of row,
// 40 px below that, and under 208 px Activities and Soundboard fold into
// a More menu. Mic and deafen live in the account row.
Rectangle {
    id: root

    readonly property bool inCall: Voice.channelId.length > 0
    readonly property bool callLive: inCall && Voice.live

    implicitHeight: inCall ? 56 + actions.height + 8 : 0
    visible: inCall
    color: Theme.ply1

    Rectangle { width: parent.width; height: 2; color: Theme.ink1 }

    // When the call came up (the web's `startedAt`), for the clock.
    property double liveSince: 0
    property double now: Date.now()
    onCallLiveChanged: liveSince = callLive ? Date.now() : 0
    Component.onCompleted: liveSince = callLive ? Date.now() : 0
    Timer {
        interval: 1000
        repeat: true
        running: root.callLive && root.visible
        onTriggered: root.now = Date.now()
    }
    readonly property var stats: JSON.parse(Calls.view).stats ?? null
    readonly property var lastSample: stats && stats.samples && stats.samples.length > 0
        ? stats.samples[stats.samples.length - 1] : null
    readonly property bool dmCall: JSON.parse(Calls.view).inDmCall === true

    function clock(ms) {
        const total = Math.max(0, Math.floor(ms / 1000))
        const h = Math.floor(total / 3600)
        const m = Math.floor(total / 60) % 60
        const sec = String(total % 60).padStart(2, "0")
        return h > 0 ? "%1:%2:%3".arg(h).arg(String(m).padStart(2, "0")).arg(sec) : "%1:%2".arg(m).arg(sec)
    }

    // VoiceDashboard.tsx: the lit word ("Voice connected" where it fits
    // whole, else "Connected"), where the call is (" · server" only when
    // the server fits whole), then ping, the call's clock and the voice
    // options key (here: the connection details).
    Item {
        x: 8
        y: 8
        width: parent.width - 14
        height: 40
        Column {
            id: place
            anchors.left: parent.left
            anchors.right: readouts.left
            anchors.rightMargin: 6
            anchors.verticalCenter: parent.verticalCenter
            leftPadding: 6
            spacing: 2
            Row {
                spacing: 8
                Led {
                    anchors.verticalCenter: parent.verticalCenter
                    color: root.callLive ? Theme.talk : Theme.idle
                }
                Text {
                    textFormat: Text.PlainText
                    text: !root.callLive ? (Voice.status || qsTr("Connecting"))
                        : place.width >= 147 ? (root.dmCall ? qsTr("Call connected") : qsTr("Voice connected"))
                        : qsTr("Connected")
                    color: root.callLive ? Theme.successInk : Theme.warningInk
                    font: Theme.type.silk
                }
            }
            TextMetrics {
                id: placeMetrics
                font: placeText.font
                text: Voice.channelName + " · " + Nav.activeServerName
            }
            Text {
                id: placeText
                width: place.width - place.leftPadding
                textFormat: Text.PlainText
                // A DM call names who it is with (UserPanel.tsx "Call with …").
                text: root.dmCall ? qsTr("Call with %1").arg(Voice.channelName)
                    : Nav.activeServerId.length > 0 && placeMetrics.advanceWidth <= width
                        ? Voice.channelName + " · " + Nav.activeServerName
                        : Voice.channelName
                elide: Text.ElideRight
                color: Theme.ink2
                font: Qt.font({ family: "Yap Geist Medium", pixelSize: 13, weight: Font.Medium })
            }
        }
        Row {
            id: readouts
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: 6
            Text {
                visible: root.callLive && root.lastSample !== null
                anchors.verticalCenter: parent.verticalCenter
                text: root.lastSample ? qsTr("%1 ms").arg(Math.round(root.lastSample[3])) : ""
                color: Theme.ink3
                font: Theme.type.numSm
            }
            Text {
                visible: root.callLive && root.liveSince > 0
                anchors.verticalCenter: parent.verticalCenter
                text: root.clock(root.now - root.liveSince)
                color: Theme.ink1
                font: Qt.font({ family: "Yap Martian Num SemiBold", pixelSize: 12, weight: Font.DemiBold })
            }
            IconKey {
                id: optionsKey
                anchors.verticalCenter: parent.verticalCenter
                size: 24
                tone: (details.item?.opened ?? false) ? "toggled" : "muted"
                iconName: "sliders"
                tip: qsTr("Voice options")
                onClicked: details.load()?.toggleAt(optionsKey)
            }
        }
    }
    LazyPopup { id: details; sourceComponent: VoiceDetailsPopup {} }

    Item {
        id: actions
        x: 8
        y: 56
        width: parent.width - 16
        height: key
        readonly property int key: width >= 248 ? 48 : 40
        readonly property bool wide: width >= 208
        Row {
            spacing: 2
            IconKey {
                size: actions.key
                glyph: Theme.iconLg
                iconName: "camera"
                tone: Voice.cameraOn ? "live" : "rest"
                // In a call only: the first read lists the cameras, which
                // starts Media Foundation (~MBs), and this panel exists,
                // hidden, all the time.
                enabled: root.inCall && Camera.hasCamera
                opacity: enabled ? 1 : 0.45
                tip: !enabled ? qsTr("No camera detected")
                    : Voice.cameraOn ? qsTr("Turn off camera") : qsTr("Turn on camera")
                Accessible.checkable: true
                Accessible.checked: Voice.cameraOn
                onClicked: ShellState.toggleCamera()
            }
            IconKey {
                size: actions.key
                glyph: Theme.iconLg
                iconName: "screenShare"
                tone: Voice.sharing ? "live" : "rest"
                tip: Voice.sharing ? qsTr("Stop sharing") : qsTr("Share screen")
                Accessible.checkable: true
                Accessible.checked: Voice.sharing
                onClicked: ShellState.toggleShare()
            }
            IconKey {
                id: activitiesKey
                visible: actions.wide
                size: actions.key
                glyph: Theme.iconLg
                iconName: "activities"
                tone: activitiesPopup.item?.opened ? "toggled" : "rest"
                tip: qsTr("Activities")
                onClicked: root.togglePopover(activitiesPopup, activitiesKey)
            }
            IconKey {
                id: soundboardKey
                visible: actions.wide
                size: actions.key
                glyph: Theme.iconLg
                iconName: "soundboard"
                tone: soundboardPopup.item?.opened ? "toggled" : "rest"
                tip: qsTr("Soundboard")
                Accessible.name: qsTr("Soundboard")
                onClicked: root.togglePopover(soundboardPopup, soundboardKey)
            }
            IconKey {
                id: moreKey
                visible: !actions.wide
                size: actions.key
                glyph: Theme.iconLg
                iconName: "more"
                tone: (moreMenu.item?.visible ?? false) || soundboardPopup.item?.opened ? "toggled" : "rest"
                tip: qsTr("More call actions")
                onClicked: {
                    const menu = moreMenu.load()
                    menu?.popup(moreKey, 0, -menu.implicitHeight - 8)
                }
            }
        }
        IconKey {
            id: leave
            anchors.right: parent.right
            size: actions.key
            glyph: Theme.iconLg
            iconName: "disconnect"
            tip: qsTr("Leave call")
            onClicked: Voice.leave()
            background: Rectangle {
                radius: Theme.radius
                color: Theme.dangerFill
                Rectangle {
                    anchors.fill: parent
                    radius: Theme.radius
                    color: Theme.dangerFillInk
                    opacity: leave.down ? 0.14 : leave.hovered ? 0.12 : 0
                }
            }
            contentItem: Item {
                Icon {
                    anchors.centerIn: parent
                    name: "disconnect"
                    size: Theme.iconLg
                    color: Theme.dangerFillInk
                }
            }
        }
    }

    // The call's apps, as the stage dock opens them (VoiceView.qml):
    // Activities opens the activities dialog (Wordle; the rest are on hold).
    property double popoverClosedAt: 0
    function togglePopover(loader, key) {
        if (Date.now() - popoverClosedAt < 300)
            return
        loader.active = true
        if (loader.status === Loader.Ready)
            loader.item.openAt(key)
    }
    onInCallChanged: {
        if (!inCall) {
            soundboardPopup.item?.close()
            activitiesPopup.item?.close()
        }
    }
    Loader {
        id: soundboardPopup
        active: false
        sourceComponent: SoundboardPicker { onClosed: root.popoverClosedAt = Date.now() }
    }
    Loader {
        id: activitiesPopup
        active: false
        sourceComponent: ActivitiesDialog {
            people: Nav.voicePanelId === Voice.channelId ? Math.max(1, Nav.voicePanelCount) : 1
            onClosed: root.popoverClosedAt = Date.now()
        }
    }
    LazyPopup {
        id: moreMenu
        sourceComponent: MenuPanel {
            MenuRow {
                glyph: "activities"
                text: qsTr("Activities")
                onTriggered: root.togglePopover(activitiesPopup, moreKey)
            }
            MenuRow {
                glyph: "soundboard"
                text: qsTr("Soundboard")
                onTriggered: root.togglePopover(soundboardPopup, moreKey)
            }
        }
    }
}
