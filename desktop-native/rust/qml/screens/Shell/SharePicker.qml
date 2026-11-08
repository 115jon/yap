import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Templates as T
import Yap.Chrome
import com.yap.native

// Share your screen (DesktopScreenPickerModal.tsx in
// ScreenSharePickerShell.tsx): the title and what this is; underline tabs
// for Applications and Entire screen; a scrolling grid of sources with
// stills (one column under 440 px, two under 680, then three); and the
// footer: stream quality and Include audio, then the picked source with its
// preset, Cancel and Go live. The list refreshes every 5 s while open so
// windows that open or close show up; the first source of the tab is picked
// until you pick one. Go live keeps the quality and audio for next time.
Popup {
    id: root

    property var sources: []
    property string selectedId: ""
    property string quality: "720p30"
    property bool withAudio: true
    // New stills on every open.
    property int nonce: 0

    readonly property bool apps: ShellState.shareTab === "applications"
    readonly property var shown: sources.filter(s => apps ? s.kind === "window" : s.kind === "monitor")
    readonly property var picked: sources.find(s => s.id === selectedId) ?? null
    readonly property bool onlyOurs: Voice.shareOnlyOurs()
    readonly property var qualityIds: ["720p30", "720p60", "1080p30", "1080p60", "1440p30", "1440p60", "4k30", "4k60"]

    function openPicker() {
        // Changing the source of a live share keeps its quality and audio.
        const base = Voice.sharing ? Voice.shareQuality : Audio.screenShareQuality
        quality = qualityIds.includes(base) ? base : "720p30"
        withAudio = Voice.sharing ? Voice.shareAudio : Audio.screenShareWithAudio
        selectedId = ""
        loading = true
        nonce++
        Voice.refreshShareSources()
        open()
    }

    // Until the first list after opening lands ("Finding what you can share").
    property bool loading: true
    // The list as last read: the tiles (and their stills) are rebuilt only
    // when it changes, not on every poll.
    property string sourcesJson: ""

    // A refresh landed (off the GUI thread; see Voice.refreshShareSources).
    function take() {
        const json = Voice.shareSources
        if (json !== sourcesJson) {
            sourcesJson = json
            try {
                sources = JSON.parse(json)
            } catch (error) {
                sources = []
            }
        }
        loading = false
        keepPick()
    }

    // The pick stays while its source is listed on this tab; otherwise the
    // tab's first source.
    function keepPick() {
        if (!shown.some(s => s.id === selectedId))
            selectedId = shown.length > 0 ? shown[0].id : ""
    }

    function goLive() {
        const source = picked
        if (!source)
            return
        Audio.changeScreenShareDefaults(quality, withAudio)
        Voice.startShare(source.id, source.name, source.kind === "window", quality, withAudio)
        close()
    }

    function presetLine() {
        const id = quality
        const label = id.startsWith("4k") ? "4K" : id.replace(/\d+$/, "")
        const fps = id.endsWith("60") ? 60 : 30
        return withAudio ? qsTr("%1 · %2 fps · with audio").arg(label).arg(fps)
            : qsTr("%1 · %2 fps").arg(label).arg(fps)
    }

    Connections {
        target: ShellState
        function onShareTabChanged() { root.keepPick() }
    }
    Connections {
        target: Voice
        function onShareSourcesRevisionChanged() { root.take() }
    }

    Timer {
        interval: 5000
        repeat: true
        running: root.visible
        onTriggered: Voice.refreshShareSources()
    }

    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(860, parent.width - 32)
    height: Math.min(640, parent.height - 32)
    modal: true
    // Ctrl+K stays shut while a modal is up (ExtrasHost).
    onVisibleChanged: ShellState.modalCount += visible ? 1 : -1
    focus: true
    padding: 0
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    Overlay.modal: Scrim { color: Theme.scrim }

    background: Rectangle {
        radius: Theme.radiusSheet
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
    }

    enter: PresenceIn { fromScale: Theme.scaleDialog }
    exit: PresenceOut { toScale: Theme.scaleDialogOut }

    contentItem: Item {
        // Header: 24 px inset, Close at the right.
        Column {
            id: header
            x: 24
            y: 20
            width: parent.width - 48 - 40
            spacing: 4
            Text {
                text: qsTr("Share your screen")
                color: Theme.ink1
                font: Theme.type.sectionTitle
                Accessible.role: Accessible.Heading
            }
            Text {
                width: parent.width
                text: qsTr("Pick an app or a screen.")
                wrapMode: Text.Wrap
                color: Theme.ink2
                font: Theme.type.description
            }
        }
        IconKey {
            anchors.right: parent.right
            anchors.rightMargin: 16
            y: 16
            size: 32
            iconName: "close"
            tip: qsTr("Close")
            onClicked: root.close()
        }

        // Underline tabs: Bricolage labels on a 1 px line, a 2 px ink-1
        // indicator under the chosen one.
        Item {
            id: tabs
            anchors.top: header.bottom
            anchors.topMargin: 12
            width: parent.width
            height: 40
            Accessible.role: Accessible.PageTabList
            Accessible.name: qsTr("What to share")
            Row {
                id: tabRow
                x: 24
                height: parent.height
                spacing: 20
                Tab { id: appsTab; value: "applications"; text: qsTr("Applications") }
                Tab { id: screensTab; value: "screens"; text: qsTr("Entire screen") }
            }
            Rectangle {
                id: indicator
                readonly property Item at: root.apps ? appsTab : screensTab
                anchors.bottom: parent.bottom
                x: tabRow.x + at.x
                width: at.width
                height: 2
                color: Theme.ink1
                Behavior on x { NumberAnimation { duration: Theme.reducedMotion ? 0 : 200; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeInOut } }
                Behavior on width { NumberAnimation { duration: Theme.reducedMotion ? 0 : 200; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeInOut } }
            }
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line; z: -1 }
        }

        // The sources.
        Flickable {
            id: list
            anchors.top: tabs.bottom
            anchors.bottom: footer.top
            width: parent.width
            contentHeight: body.height
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {}
            Accessible.role: Accessible.PageTab
            Accessible.name: root.apps ? appsTab.text : screensTab.text

            Item {
                id: body
                readonly property int columns: width >= 680 ? 3 : width >= 440 ? 2 : 1
                readonly property real tileWidth: (width - 12 * (columns - 1)) / columns
                x: 24
                width: list.width - 48
                height: Math.max(list.height, grid.height + 32)

                Grid {
                    id: grid
                    y: 16
                    visible: !root.loading && root.shown.length > 0
                    columns: body.columns
                    spacing: 12
                    Repeater {
                        model: root.shown
                        delegate: ShareSourceTile {
                            required property var modelData
                            width: body.tileWidth
                            sourceId: modelData.id
                            name: modelData.name
                            kind: modelData.kind
                            ours: modelData.ours
                            nonce: root.nonce
                            stillAllowed: !root.onlyOurs || modelData.ours
                            selected: root.selectedId === modelData.id
                            onClicked: root.selectedId = modelData.id
                        }
                    }
                }

                // Looking (the first list after opening).
                Column {
                    visible: root.loading
                    anchors.centerIn: parent
                    spacing: 8
                    Icon {
                        id: finding
                        anchors.horizontalCenter: parent.horizontalCenter
                        name: "loader"
                        smooth: true
                        size: Theme.iconXl
                        color: Theme.ink3
                        RotationAnimator on rotation {
                            from: 0
                            to: 360
                            duration: Theme.durSpin
                            loops: Animation.Infinite
                            running: finding.visible && !Theme.reducedMotion
                        }
                    }
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: qsTr("Finding what you can share")
                        color: Theme.ink2
                        font: Theme.type.captionStrong
                        Accessible.role: Accessible.StaticText
                    }
                }

                // Nothing to share on this tab.
                Column {
                    visible: !root.loading && root.shown.length === 0
                    anchors.centerIn: parent
                    width: Math.min(parent.width, 360)
                    spacing: 8
                    Icon {
                        anchors.horizontalCenter: parent.horizontalCenter
                        name: root.apps ? "window" : "monitor"
                        size: Theme.icon2xl
                        color: Theme.ink3
                    }
                    Text {
                        width: parent.width
                        horizontalAlignment: Text.AlignHCenter
                        textFormat: Text.PlainText
                        text: root.apps ? qsTr("No open apps to share") : qsTr("No screens found")
                        color: Theme.ink1
                        font: Theme.type.panelTitle
                    }
                    Text {
                        visible: root.apps
                        width: parent.width
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.Wrap
                        text: qsTr("Open the app you want to share, or share an entire screen instead.")
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                    DialogKey {
                        visible: root.apps
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: qsTr("Share entire screen")
                        onClicked: ShellState.shareTab = "screens"
                    }
                }
            }
        }

        // Footer: quality and audio, a rule, then the pick and the keys.
        Column {
            id: footer
            anchors.bottom: parent.bottom
            width: parent.width
            Rectangle { width: parent.width; height: 1; color: Theme.line }
            Item {
                x: 24
                width: parent.width - 48
                height: Math.max(qualityControl.implicitHeight, audioRow.implicitHeight) + 24
                QualityControl {
                    id: qualityControl
                    y: 12
                    value: root.quality
                    onPicked: id => root.quality = id
                }
                T.AbstractButton {
                    id: audioRow
                    HoverHandler { cursorShape: Qt.PointingHandCursor }
                    anchors.right: parent.right
                    y: 12
                    implicitWidth: audioLabel.implicitWidth + 16 + 44
                    implicitHeight: 28
                    focusPolicy: Qt.StrongFocus
                    Accessible.role: Accessible.CheckBox
                    Accessible.checkable: true
                    Accessible.checked: root.withAudio
                    Accessible.name: qsTr("Include audio")
                    onClicked: root.withAudio = !root.withAudio
                    contentItem: Row {
                        spacing: 6
                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: "voiceChannel"
                            size: Theme.iconSm
                            color: Theme.ink3
                        }
                        Text {
                            id: audioLabel
                            anchors.verticalCenter: parent.verticalCenter
                            text: qsTr("Include audio")
                            color: Theme.ink1
                            font: Theme.type.row
                        }
                    }
                    // The switch: a 36 x 20 track, ink fill when on.
                    Rectangle {
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        width: 36
                        height: 20
                        radius: 10
                        color: root.withAudio ? Theme.primaryFill : Theme.tint3
                        Behavior on color { ColorAnimation { duration: Theme.dur1 } }
                        Rectangle {
                            x: root.withAudio ? parent.width - width - 2 : 2
                            anchors.verticalCenter: parent.verticalCenter
                            width: 16
                            height: 16
                            radius: 8
                            color: root.withAudio ? Theme.primaryFillInk : Theme.ply4
                            Behavior on x { NumberAnimation { duration: Theme.reducedMotion ? 0 : Theme.dur1 } }
                        }
                        Rectangle {
                            anchors.fill: parent
                            anchors.margins: -3
                            visible: audioRow.visualFocus
                            color: "transparent"
                            radius: 13
                            border.width: 2
                            border.color: Theme.focus
                        }
                    }
                }
            }
            Rectangle { x: 24; width: parent.width - 48; height: 1; color: Theme.line }
            Item {
                x: 24
                width: parent.width - 48
                height: Theme.control + 28
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - keys.width - 16
                    spacing: 2
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: root.picked ? root.picked.name : qsTr("Select a source")
                        elide: Text.ElideRight
                        color: root.picked ? Theme.ink1 : Theme.ink3
                        font: root.picked ? Theme.type.panelTitle : Theme.type.row
                    }
                    Text {
                        visible: root.picked !== null
                        textFormat: Text.PlainText
                        text: root.presetLine()
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                }
                Row {
                    id: keys
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 8
                    DialogKey {
                        text: qsTr("Cancel")
                        onClicked: root.close()
                    }
                    DialogKey {
                        primary: true
                        enabled: root.picked !== null
                        text: qsTr("Go live")
                        onClicked: root.goLive()
                    }
                }
            }
        }
    }

    // An underline tab (Tabs.tsx `rule`): Bricolage 14/600, ink-1 when
    // chosen, ink-2 otherwise.
    component Tab: T.AbstractButton {
        id: tab
        required property string value
        readonly property bool current: ShellState.shareTab === value
        height: 40
        implicitWidth: tabLabel.implicitWidth
        focusPolicy: Qt.StrongFocus
        Accessible.role: Accessible.PageTab
        Accessible.name: text
        Accessible.checkable: true
        Accessible.checked: current
        onClicked: ShellState.shareTab = value
        Keys.onLeftPressed: ShellState.shareTab = "applications"
        Keys.onRightPressed: ShellState.shareTab = "screens"
        contentItem: Text {
            id: tabLabel
            textFormat: Text.PlainText
            text: tab.text
            verticalAlignment: Text.AlignVCenter
            color: tab.current || tab.hovered ? Theme.ink1 : Theme.ink2
            EaseColor on color {}
            font: Theme.type.controlSm
        }
        background: Rectangle {
            anchors.fill: parent
            anchors.margins: -2
            visible: tab.visualFocus
            color: "transparent"
            radius: Theme.radius
            border.width: 2
            border.color: Theme.focus
        }
        HoverHandler { cursorShape: Qt.PointingHandCursor }
    }
}
