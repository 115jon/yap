pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The open channel's threads, a page over the pager (the desktop member
// list's Threads tab, phone layout): by latest reply, loaded afresh each
// time the page shows (back from a thread too). The floating bar (Drawer Plus) holds the back key,
// the channel over "Threads" and the count; each thread is a row with a
// seam below: the starter's avatar and name, up to two lines of the
// message, then the replies and when the last one came. A tap opens the
// thread (ThreadPage). Loading shows placeholder rows; a failure says why
// with Try again; none says so.
Rectangle {
    id: root

    readonly property bool sheet: false
    property real backProgress: 0

    signal closeRequested()

    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: Math.max(SafeArea.margins.bottom, Native.gestureInset)

    // `[{id, author, initials, avatar, content, replies, lastReply}]`
    // (yap-ui panels.rs `Panels.threads`).
    readonly property var rows: {
        try {
            return JSON.parse(Panels.threads || "[]")
        } catch (e) {
            return []
        }
    }
    readonly property bool failed: Panels.threadsError.length > 0
    readonly property bool ready: !Panels.threadsLoading && !failed

    function load() {
        Panels.listThreads(Date.now())
    }
    Component.onCompleted: load()
    // Back from a thread: its replies may have changed.
    property bool shownOnce: false
    StackView.onActivated: {
        if (shownOnce)
            load()
        shownOnce = true
    }

    color: Theme.ply0
    transform: [
        Scale {
            origin.x: root.width / 2
            origin.y: root.height / 2
            xScale: 1 - root.backProgress * 0.08
            yScale: 1 - root.backProgress * 0.08
        },
        Translate { x: root.backProgress * root.width * 0.1 }
    ]

    // The bar: back, the channel over the title, the count.
    Item {
        id: bar
        x: 8 + root.leftInset
        y: root.topInset + 8
        width: parent.width - 16 - root.leftInset - root.rightInset
        height: Math.max(56, Math.ceil(kicker.implicitHeight + title.implicitHeight + 12))
        z: 3

        FloatPlate {
            anchors.fill: parent
        }
        IconKey {
            id: backKey
            anchors.left: parent.left
            anchors.leftMargin: 4
            anchors.verticalCenter: parent.verticalCenter
            iconName: "back"
            tip: qsTr("Back to the conversation")
            onClicked: root.closeRequested()
        }
        Column {
            anchors.left: backKey.right
            anchors.leftMargin: 4
            anchors.right: tally.left
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            Text {
                id: kicker
                width: parent.width
                textFormat: Text.PlainText
                text: "#" + Nav.activeChannelName
                elide: Text.ElideRight
                color: Theme.ink3
                font: Theme.type.silk
                Accessible.ignored: true
            }
            Text {
                id: title
                width: parent.width
                textFormat: Text.PlainText
                text: qsTr("Threads")
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.columnTitle
                Accessible.role: Accessible.Heading
                Accessible.name: qsTr("Threads in %1").arg(Nav.activeChannelName)
            }
        }
        Text {
            id: tally
            anchors.right: parent.right
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            visible: root.ready && root.rows.length > 0
            textFormat: Text.PlainText
            text: root.rows.length
            color: Theme.ink3
            font: Theme.type.numSm
            Accessible.name: root.rows.length === 1 ? qsTr("1 thread") : qsTr("%1 threads").arg(root.rows.length)
        }
    }

    // A solid band under the status bar: the rows scroll behind the bar's
    // plate, never behind the clock and the system's icons.
    Rectangle {
        width: parent.width
        height: root.topInset
        z: 2
        color: Theme.ply0
    }

    readonly property real listTop: bar.y + bar.height + 8

    PlatformList {
        id: list
        anchors.fill: parent
        anchors.leftMargin: root.leftInset
        anchors.rightMargin: root.rightInset
        topMargin: root.listTop
        bottomMargin: root.bottomInset + 16
        visible: root.ready && count > 0
        model: root.rows
        Accessible.role: Accessible.List
        Accessible.name: qsTr("Threads")

        delegate: Rectangle {
            id: row

            required property var modelData
            readonly property int replies: modelData.replies ?? 0
            readonly property string repliesText: replies === 1 ? qsTr("1 reply") : qsTr("%1 replies").arg(replies)
            readonly property bool hasText: String(modelData.content ?? "").trim().length > 0

            width: ListView.view.width
            implicitHeight: Math.max(Theme.hitMin, column.y + column.height + 12) + 1
            color: tap.pressed ? Theme.tint1 : "transparent"
            Behavior on color { ColorAnimation { duration: Theme.dur1 } }
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Thread by %1, %2").arg(modelData.author).arg(repliesText)
            Accessible.description: modelData.content ?? ""
            Accessible.onPressAction: root.open(modelData.id)

            TapHandler {
                id: tap
                gesturePolicy: TapHandler.ReleaseWithinBounds
                onTapped: {
                    Native.haptic(Native.Tick)
                    root.open(row.modelData.id)
                }
            }

            Avatar {
                x: 16
                y: 12
                size: 36
                initials: row.modelData.initials ?? ""
                source: row.modelData.avatar ?? ""
                Accessible.ignored: true
            }

            Column {
                id: column
                x: 16 + 36 + 12
                y: 12
                width: row.width - x - 16
                spacing: 2

                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: row.modelData.author ?? ""
                    elide: Text.ElideRight
                    color: Theme.ink1
                    font: Theme.type.author
                }
                Text {
                    visible: row.hasText
                    width: parent.width
                    textFormat: Text.PlainText
                    text: row.modelData.content ?? ""
                    wrapMode: Text.Wrap
                    maximumLineCount: 2
                    elide: Text.ElideRight
                    color: Theme.ink2
                    font: Theme.type.message
                }
                Row {
                    spacing: 6
                    topPadding: 4
                    Icon {
                        anchors.verticalCenter: parent.verticalCenter
                        name: "thread"
                        size: Theme.iconSm
                        color: Theme.ink3
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: row.repliesText
                        color: Theme.ink2
                        font: Theme.type.captionStrong
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: String(row.modelData.lastReply ?? "").length > 0
                        textFormat: Text.PlainText
                        text: row.modelData.lastReply ?? ""
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                }
            }

            Rectangle {
                anchors.bottom: parent.bottom
                x: 16
                width: parent.width - 16
                height: 1
                color: Theme.line
            }
        }
    }

    // A thread over this page; Back comes here again.
    function open(messageId) {
        if (messageId.length > 0)
            Router.open("ThreadPage", { rootId: messageId }, { key: messageId })
    }

    // Loading: the row's anatomy (the avatar, a name line, two lines).
    Column {
        visible: Panels.threadsLoading
        x: root.leftInset
        y: root.listTop
        width: parent.width - root.leftInset - root.rightInset
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("Loading")
        Repeater {
            model: 4
            delegate: Item {
                id: skeleton
                required property int index
                width: parent.width
                height: 84
                Rectangle {
                    x: 16
                    y: 12
                    width: 36
                    height: 36
                    radius: Theme.radius
                    color: Theme.tint2
                }
                Column {
                    x: 16 + 36 + 12
                    y: 16
                    width: parent.width - x - 16
                    spacing: 8
                    Rectangle { width: 112; height: 12; radius: Theme.radius; color: Theme.tint2 }
                    Rectangle {
                        width: skeleton.index % 2 === 1 ? parent.width / 2 : parent.width * 3 / 4
                        height: 12
                        radius: Theme.radius
                        color: Theme.tint2
                    }
                    Rectangle { width: 72; height: 12; radius: Theme.radius; color: Theme.tint2 }
                }
                Rectangle {
                    anchors.bottom: parent.bottom
                    x: 16
                    width: parent.width - 16
                    height: 1
                    color: Theme.line
                }
            }
        }
    }

    // A failure: why, and Try again.
    Column {
        visible: !Panels.threadsLoading && root.failed
        x: 32 + root.leftInset
        y: root.listTop + 48
        width: parent.width - 64 - root.leftInset - root.rightInset
        spacing: 12
        Accessible.role: Accessible.AlertMessage
        Accessible.name: Panels.threadsError
        Icon {
            anchors.horizontalCenter: parent.horizontalCenter
            name: "offline"
            size: Theme.icon2xl
            color: Theme.ink3
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            textFormat: Text.PlainText
            text: qsTr("Couldn't load the threads")
            color: Theme.ink1
            font: Theme.type.sectionTitle
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            textFormat: Text.PlainText
            text: Panels.threadsError
            wrapMode: Text.Wrap
            color: Theme.ink2
            font: Theme.type.description
        }
        KitButton {
            anchors.horizontalCenter: parent.horizontalCenter
            variant: "secondary"
            iconName: "refresh"
            text: qsTr("Try again")
            onClicked: root.load()
        }
    }

    // None.
    Column {
        visible: root.ready && root.rows.length === 0
        x: 32 + root.leftInset
        y: root.listTop + 48
        width: parent.width - 64 - root.leftInset - root.rightInset
        spacing: 4
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("No threads in this channel")
        Item {
            width: parent.width
            height: 60 + 12
            Image {
                anchors.horizontalCenter: parent.horizontalCenter
                width: 26 * 3
                height: 20 * 3
                smooth: false
                source: "image://dither/empty/" + Theme.hex(Theme.ink3) + "/" + Theme.hex(Theme.accentInk)
                Accessible.ignored: true
            }
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            textFormat: Text.PlainText
            text: qsTr("No threads in this channel")
            color: Theme.ink1
            font: Theme.type.sectionTitle
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            textFormat: Text.PlainText
            text: qsTr("Long-press a message and choose Create thread to start one.")
            wrapMode: Text.Wrap
            color: Theme.ink2
            font: Theme.type.description
        }
    }
}
