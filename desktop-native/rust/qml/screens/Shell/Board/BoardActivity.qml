pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// The Activity tab (ProfileActivity.tsx): games, voice, streams, Listen
// Together, badges and servers joined, grouped by day (Today, Yesterday,
// "Monday 24 Nov"), newest first, with a kind filter and "Show more". The
// owner also sets how long it is kept (7, 30 or 90 days), which kinds only
// they see, and hides single events (shown faded with "Hidden").
Column {
    id: root

    property var view: ({})
    property string ownerName: ""
    readonly property bool owner: view?.isOwner ?? false
    readonly property var days: view?.days ?? []
    readonly property var settings: view?.settings ?? null
    readonly property var icons: ({ game: "gamepad", voice: "mic", stream: "stream", listen: "headphones", badge: "verified", server_join: "server" })

    spacing: 16

    // Filter chips.
    Flow {
        width: parent.width
        spacing: 6
        Repeater {
            model: [{ id: "", label: qsTr("All") }].concat(root.view?.kinds ?? [])
            delegate: Rectangle {
                id: chip
                required property var modelData
                readonly property bool on: (root.view?.kind ?? "") === modelData.id
                width: label.implicitWidth + 20
                height: 28
                radius: Theme.radius
                color: on ? Theme.primaryFill : chipHover.hovered ? Theme.tint2 : Theme.tint1
                Accessible.role: Accessible.RadioButton
                Accessible.name: modelData.label
                Accessible.checked: on
                Text {
                    textFormat: Text.PlainText
                    id: label
                    anchors.centerIn: parent
                    text: chip.modelData.label
                    color: chip.on ? Theme.primaryFillInk : Theme.ink1
                    font: Theme.type.controlSm
                }
                HoverHandler { id: chipHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: Board.activityKind(chip.modelData.id) }
            }
        }
    }

    // Owner: retention and the kinds only they see.
    Rectangle {
        visible: root.owner && root.settings !== null
        width: parent.width
        height: ownerColumn.implicitHeight + 24
        radius: Theme.radius
        color: Theme.ply2
        border.width: 1
        border.color: Theme.line
        Column {
            id: ownerColumn
            x: 12
            y: 12
            width: parent.width - 24
            spacing: 10
            Row {
                spacing: 8
                Text {
                    textFormat: Text.PlainText
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Keep for")
                    color: Theme.ink2
                    font: Theme.type.silk
                }
                Repeater {
                    model: [7, 30, 90]
                    delegate: Rectangle {
                        id: keep
                        required property int modelData
                        readonly property bool on: root.settings?.retention === modelData
                        width: keepText.implicitWidth + 16
                        height: 24
                        radius: Theme.radius
                        color: on ? Theme.primaryFill : keepHover.hovered ? Theme.tint2 : Theme.tint1
                        Accessible.role: Accessible.RadioButton
                        Accessible.name: qsTr("%1 days").arg(modelData)
                        Accessible.checked: on
                        Text {
                            textFormat: Text.PlainText
                            id: keepText
                            anchors.centerIn: parent
                            text: qsTr("%1 days").arg(keep.modelData)
                            color: keep.on ? Theme.primaryFillInk : Theme.ink1
                            font: Theme.type.caption
                        }
                        HoverHandler { id: keepHover; cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: Board.setRetention(keep.modelData) }
                    }
                }
            }
            Text {
                textFormat: Text.PlainText
                text: qsTr("Only you see")
                color: Theme.ink2
                font: Theme.type.silk
            }
            Flow {
                width: parent.width
                spacing: 6
                Repeater {
                    model: root.view?.kinds ?? []
                    delegate: Rectangle {
                        id: hideChip
                        required property var modelData
                        readonly property bool on: (root.settings?.hidden ?? []).indexOf(modelData.id) >= 0
                        width: hideRow.implicitWidth + 16
                        height: 24
                        radius: Theme.radius
                        color: hideHover.hovered ? Theme.tint2 : Theme.tint1
                        border.width: on ? 1 : 0
                        border.color: Theme.accentInk
                        Accessible.role: Accessible.CheckBox
                        Accessible.name: qsTr("%1 only for me").arg(modelData.label)
                        Accessible.checked: on
                        Row {
                            id: hideRow
                            anchors.centerIn: parent
                            spacing: 4
                            Icon {
                                anchors.verticalCenter: parent.verticalCenter
                                name: hideChip.on ? "eyeOff" : "eye"
                                size: Theme.iconXs
                                color: hideChip.on ? Theme.accentInk : Theme.ink3
                            }
                            Text {
                                textFormat: Text.PlainText
                                text: hideChip.modelData.label
                                color: Theme.ink1
                                font: Theme.type.caption
                            }
                        }
                        HoverHandler { id: hideHover; cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: Board.hideKind(hideChip.modelData.id, !hideChip.on) }
                    }
                }
            }
        }
    }

    Text {
        textFormat: Text.PlainText
        visible: (root.view?.loading ?? false) && !(root.view?.loaded ?? false)
        text: qsTr("Loading…")
        color: Theme.ink3
        font: Theme.type.description
    }
    Column {
        visible: (root.view?.error ?? "").length > 0
        width: parent.width
        spacing: 8
        Text {
            textFormat: Text.PlainText
            width: parent.width
            text: root.view?.error ?? ""
            wrapMode: Text.Wrap
            color: Theme.ink2
            font: Theme.type.description
        }
        KitButton {
            size: "sm"
            variant: "secondary"
            iconName: "refresh"
            text: qsTr("Try again")
            onClicked: Board.retry()
        }
    }
    Text {
        textFormat: Text.PlainText
        visible: (root.view?.loaded ?? false) && root.days.length === 0
        width: parent.width
        text: root.owner ? qsTr("Nothing here yet. Games, calls, streams, Listen Together and badges show up as they happen.")
            : qsTr("Nothing from %1 here yet.").arg(root.ownerName)
        wrapMode: Text.Wrap
        color: Theme.ink3
        font: Theme.type.description
    }

    Repeater {
        model: root.days
        delegate: Column {
            id: day
            required property var modelData
            width: root.width
            spacing: 2
            Row {
                width: parent.width
                height: 24
                Text {
                    textFormat: Text.PlainText
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - n.implicitWidth
                    text: day.modelData.label
                    color: Theme.ink3
                    font: Theme.type.silk
                    Accessible.role: Accessible.Heading
                }
                Text {
                    textFormat: Text.PlainText
                    id: n
                    anchors.verticalCenter: parent.verticalCenter
                    text: String(day.modelData.events.length)
                    color: Theme.ink3
                    font: Theme.type.numSm
                }
            }
            Repeater {
                model: day.modelData.events
                delegate: Item {
                    id: event
                    required property var modelData
                    width: day.width
                    height: 48
                    opacity: modelData.hidden ? 0.55 : 1
                    Accessible.role: Accessible.ListItem
                    Accessible.name: modelData.title + (modelData.detail ? ", " + modelData.detail : "") + ", " + modelData.time
                    HoverHandler { id: eventHover }
                    Rectangle {
                        anchors.fill: parent
                        radius: Theme.radius
                        color: eventHover.hovered ? Theme.tint1 : "transparent"
                    }
                    Rectangle {
                        x: 4
                        anchors.verticalCenter: parent.verticalCenter
                        width: 32
                        height: 32
                        radius: Theme.radius
                        color: Theme.tint2
                        clip: true
                        Icon {
                            anchors.centerIn: parent
                            visible: art.status !== Image.Ready
                            name: root.icons[event.modelData.kind] ?? "sparkle"
                            size: Theme.iconSm
                            color: Theme.ink2
                        }
                        Image {
                            id: art
                            anchors.fill: parent
                            source: event.modelData.icon ?? ""
                            sourceSize.width: 64
                            fillMode: Image.PreserveAspectCrop
                            asynchronous: true
                            cache: true
                        }
                    }
                    Column {
                        x: 48
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - 48 - right.width - 12
                        Row {
                            spacing: 6
                            Text {
                                textFormat: Text.PlainText
                                text: event.modelData.title
                                elide: Text.ElideRight
                                width: Math.min(implicitWidth, event.width - 48 - right.width - 80)
                                color: Theme.ink1
                                font: Theme.type.row
                            }
                            Text {
                                textFormat: Text.PlainText
                                visible: event.modelData.hidden
                                anchors.baseline: parent.children[0].baseline
                                text: qsTr("Hidden")
                                color: Theme.accentInk
                                font: Theme.type.silk
                            }
                        }
                        Text {
                            visible: text.length > 0
                            width: parent.width
                            textFormat: Text.PlainText
                            text: event.modelData.detail
                            elide: Text.ElideRight
                            color: Theme.ink3
                            font: Theme.type.caption
                        }
                    }
                    Row {
                        id: right
                        anchors.right: parent.right
                        anchors.rightMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 10
                        Text {
                            textFormat: Text.PlainText
                            anchors.verticalCenter: parent.verticalCenter
                            visible: text.length > 0
                            text: event.modelData.duration
                            color: Theme.ink2
                            font: Theme.type.numSm
                        }
                        Text {
                            textFormat: Text.PlainText
                            anchors.verticalCenter: parent.verticalCenter
                            text: event.modelData.time
                            color: Theme.ink3
                            font: Theme.type.numSm
                        }
                        IconKey {
                            visible: root.owner && (eventHover.hovered || hovered)
                            anchors.verticalCenter: parent.verticalCenter
                            size: 24
                            iconName: event.modelData.hidden ? "eye" : "eyeOff"
                            tip: event.modelData.hidden ? qsTr("Show to everyone") : qsTr("Hide from others")
                            onClicked: Board.hideEvent(event.modelData.id, !event.modelData.hidden)
                        }
                    }
                }
            }
        }
    }

    KitButton {
        visible: root.view?.more ?? false
        size: "sm"
        variant: "secondary"
        text: qsTr("Show more")
        loading: root.view?.loading ?? false
        onClicked: Board.activityMore()
    }
}
