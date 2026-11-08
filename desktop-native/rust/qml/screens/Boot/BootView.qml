import QtQuick
import com.yap.native

// The cold load (chat/ColdLoad.tsx, wider screens): a compact plate in the
// middle of the window over a dimmed, still outline of the app, so it reads
// as an overlay over the app that is coming. Yappy barks while the steps run
// over a three-cell step meter, a silkscreen kicker and one Bricolage line
// that changes as the steps complete. Past 10 s it says the server is slow;
// with no network (the last try never left the device) it waits for one.
// When the app is ready the mark settles into the logo, the line says
// "You're in", and the layer holds, then fades (rm-loading-exit, 1.2 s) over
// the live app. The bark is the only loop, and only while loading.
Item {
    id: root

    required property string status
    // Booting: the steps are running.
    required property bool active
    // The exit fade is running (the caller keeps the layer shown).
    readonly property bool exiting: exit.running

    // 0 signing in (until /me), 1 opening chat (the gateway connecting),
    // 2 loading messages (servers and the page), 3 ready (the exit).
    readonly property int step: !active ? 3
        : status.indexOf("servers") >= 0 ? 2
        : Session.meId.length > 0 && !Nav.connected ? 1 : 0
    // LOADER_SETTLE_MS: still loading after this long reads as slow.
    property bool slow: false
    readonly property var lines: [
        { line: qsTr("Signing you in"), detail: qsTr("Checking your session on this device.") },
        { line: qsTr("Opening chat"), detail: qsTr("Connecting to the chat server.") },
        { line: qsTr("Loading messages"), detail: qsTr("Fetching your servers and what you missed.") },
        { line: qsTr("You're in"), detail: qsTr("Welcome back.") }
    ]
    // ColdLoad.tsx `error`: offline outranks slow (useOnline).
    readonly property bool showOffline: Nav.linkOffline && step < 3
    readonly property bool showSlow: slow && step < 3 && !showOffline
    readonly property bool warned: showOffline || showSlow
    readonly property var copy: showOffline
        ? { line: qsTr("Waiting for network"), detail: qsTr("Yap connects on its own as soon as this device is back online.") }
        : showSlow
        ? { line: qsTr("Taking longer than usual"), detail: qsTr("The server is slow to answer. Yap keeps trying on its own.") }
        : lines[step]
    readonly property string kicker: step >= 3 ? qsTr("03 / 03 · Ready")
        : "0" + (step + 1) + " / 03" + (showOffline ? " · " + qsTr("Offline") : showSlow ? " · " + qsTr("Slow") : "")

    onActiveChanged: {
        if (active) {
            exit.stop()
            opacity = 1
            slow = false
            slowTimer.restart()
        } else if (Session.phase === Session.Ready) {
            exit.restart()
        }
    }
    Component.onCompleted: if (active) slowTimer.start()

    Timer {
        id: slowTimer
        interval: 10000
        onTriggered: root.slow = true
    }

    // Hold "all three done", then fade: 0-60 % at full, 60-100 % out.
    SequentialAnimation {
        id: exit
        PauseAnimation { duration: Theme.reducedMotion ? 0 : 720 }
        OpacityAnimator {
            target: root
            from: 1
            to: 0
            duration: Theme.reducedMotion ? 100 : 480
            easing.type: Easing.BezierSpline
            easing.bezierCurve: Theme.easeOut
        }
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.ply0
    }

    // ShellOutline: the rail, the channel column and the chat, in tint-1
    // blocks at 60 %. No shimmer. The web's own 32 px bar band is the
    // native title bar here.
    Item {
        id: outline
        anchors.fill: parent
        opacity: 0.6
        Accessible.ignored: true

        readonly property var rows: [72, 56, 64, 48, 80, 60]

        Column {
            x: 10
            y: 12
            spacing: 8
            Repeater {
                model: 4
                delegate: Rectangle { width: 40; height: 40; radius: Theme.radius; color: Theme.tint1 }
            }
        }

        Rectangle {
            id: column
            x: Theme.rail
            width: Theme.treePanel
            height: parent.height
            color: Theme.ply1
            Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Theme.line }
            Item {
                width: parent.width - 1
                height: Theme.header
                Rectangle { x: 16; anchors.verticalCenter: parent.verticalCenter; width: 112; height: 16; radius: Theme.radius; color: Theme.tint1 }
                Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line }
            }
            Column {
                x: 8
                y: Theme.header + 16
                spacing: 2
                Item {
                    width: 96 + 8
                    height: 12 + 12
                    Rectangle { x: 8; width: 96; height: 12; radius: Theme.radius; color: Theme.tint1 }
                }
                Repeater {
                    model: outline.rows.slice(0, 5)
                    delegate: Item {
                        required property int modelData
                        width: 240
                        height: 32
                        Rectangle { x: 8; anchors.verticalCenter: parent.verticalCenter; width: 16; height: 16; radius: Theme.radius; color: Theme.tint1 }
                        Rectangle { x: 32; anchors.verticalCenter: parent.verticalCenter; width: modelData + 24; height: 12; radius: Theme.radius; color: Theme.tint1 }
                    }
                }
            }
        }

        Item {
            x: column.x + column.width
            width: parent.width - x
            height: parent.height
            Item {
                width: parent.width
                height: Theme.header
                Rectangle { x: 16; anchors.verticalCenter: parent.verticalCenter; width: 20; height: 20; radius: Theme.radius; color: Theme.tint1 }
                Rectangle { x: 48; anchors.verticalCenter: parent.verticalCenter; width: 128; height: 16; radius: Theme.radius; color: Theme.tint1 }
                Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line }
            }
            Column {
                x: 16
                anchors.bottom: composerOutline.top
                anchors.bottomMargin: 16
                width: parent.width - 32
                spacing: 24
                Repeater {
                    model: outline.rows
                    delegate: Item {
                        id: line
                        required property int modelData
                        width: parent.width
                        height: 40
                        Rectangle { width: 40; height: 40; radius: Theme.radius; color: Theme.tint1 }
                        Rectangle { x: 52; y: 4; width: 96; height: 12; radius: Theme.radius; color: Theme.tint1 }
                        Rectangle { x: 52; y: 24; width: (line.width - 52) * line.modelData / 100; height: 12; radius: Theme.radius; color: Theme.tint1 }
                    }
                }
            }
            Rectangle {
                id: composerOutline
                x: 16
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 16
                width: parent.width - 32
                height: 48
                radius: Theme.radius
                color: Theme.ply2
                border.width: 1
                border.color: Theme.line
            }
        }
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.scrim
    }

    Rectangle {
        id: plate
        anchors.horizontalCenter: parent.horizontalCenter
        y: Math.max(24, parent.height / 2 - 190)
        width: 400
        height: content.implicitHeight + 64
        radius: Theme.radius
        color: Theme.ply0
        border.width: 1
        border.color: Theme.lineStrong
        Rectangle { width: parent.width; height: 2; color: Theme.ink1 }

        Column {
            id: content
            x: 32
            y: 32
            width: parent.width - 64
            spacing: 0

            // LogoResolve: the bark, then the real mark over it once ready.
            Item {
                anchors.horizontalCenter: parent.horizontalCenter
                width: bark.width
                height: bark.height
                Accessible.ignored: true
                YapBark {
                    id: bark
                    cell: 6
                    barking: root.active
                    opacity: root.step >= 3 ? 0 : 1
                    Behavior on opacity { NumberAnimation { duration: Theme.reducedMotion ? 0 : 500; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
                }
                Image {
                    anchors.fill: parent
                    source: "qrc:/svg/splash-logo.svg"
                    sourceSize: Qt.size(32, 32)
                    smooth: false
                    opacity: root.step >= 3 ? 1 : 0
                    Behavior on opacity { NumberAnimation { duration: Theme.reducedMotion ? 0 : 500; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
                }
            }
            Row {
                topPadding: 32
                spacing: 12
                Row {
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4
                    Repeater {
                        model: 3
                        delegate: Rectangle {
                            required property int index
                            width: 24
                            height: 2
                            color: index < root.step ? Theme.ink1
                                : index === root.step ? (root.warned ? Theme.warningInk : Theme.ink3)
                                : Theme.lineStrong
                        }
                    }
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: root.kicker
                    color: root.warned ? Theme.warningInk : Theme.ink3
                    font: Theme.type.silk
                }
            }
            Text {
                topPadding: 12
                width: parent.width
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: root.copy.line
                color: Theme.ink1
                font.family: "Yap Bricolage Title"
                font.pixelSize: 30
                font.weight: Font.ExtraBold
                font.letterSpacing: -0.9
                lineHeight: 32
                lineHeightMode: Text.FixedHeight
                Accessible.role: Accessible.StaticText
            }
            Text {
                topPadding: 8
                width: parent.width
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: root.copy.detail
                color: Theme.ink2
                font: Theme.type.description
            }
        }
    }
}
