import QtQuick
import com.yap.native
import Yap.Ui

// The profile card's "Playing a game" (ProfileCard.tsx, GameActivityCard):
// the game's 40 px icon (left out when it has none or it fails, so the
// text takes the room), its name, Rich Presence details and state when present, and a
// live elapsed time. `game` is the core's JSON `{name, icon, details,
// state, startedAt}`, or empty (the section hides). The shared GameClock
// ticks only while this shows.
Column {
    id: root

    property string game: ""
    readonly property var info: {
        if (game.length === 0)
            return null
        try {
            return JSON.parse(game)
        } catch (e) {
            return null
        }
    }
    readonly property bool ticking: visible && info !== null
    property bool holding: false

    visible: info !== null
    width: parent ? parent.width : 0
    spacing: 8

    onTickingChanged: {
        if (ticking !== holding) {
            holding = ticking
            GameClock.hold(ticking)
        }
    }
    Component.onCompleted: if (ticking) { holding = true; GameClock.hold(true) }
    Component.onDestruction: if (holding) GameClock.hold(false)

    Text {
        text: qsTr("Playing a game")
        color: Theme.ink2
        font: Theme.type.captionStrong
        Accessible.role: Accessible.Heading
    }

    Row {
        width: parent.width
        spacing: 12

        Rectangle {
            id: tile
            visible: picture.status === Image.Ready
            width: 40
            height: 40
            radius: Theme.radius
            color: Theme.tint2
            clip: true
            Accessible.ignored: true
            Image {
                id: picture
                anchors.fill: parent
                source: root.info?.icon ?? ""
                sourceSize.width: 80
                sourceSize.height: 80
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                cache: true
            }
        }

        Column {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - (tile.visible ? 52 : 0)
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: root.info?.name ?? ""
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.nameSm
            }
            Text {
                width: parent.width
                visible: text.length > 0
                textFormat: Text.PlainText
                text: root.info?.details ?? ""
                elide: Text.ElideRight
                color: Theme.ink2
                font: Theme.type.caption
            }
            Text {
                width: parent.width
                visible: text.length > 0
                textFormat: Text.PlainText
                text: root.info?.state ?? ""
                elide: Text.ElideRight
                color: Theme.ink2
                font: Theme.type.caption
            }
            Row {
                spacing: 4
                Text {
                    anchors.baseline: word.baseline
                    textFormat: Text.PlainText
                    text: root.info ? GameClock.elapsed(GameClock.now - root.info.startedAt) : ""
                    color: Theme.ink2
                    font: Theme.type.numSm
                }
                Text {
                    id: word
                    textFormat: Text.PlainText
                    text: qsTr("elapsed")
                    color: Theme.ink2
                    font: Theme.type.caption
                }
            }
        }
    }
}
