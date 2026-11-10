import QtQuick
import com.yap.native

// Last match, S (Deadlock.tsx LastMatchSmall): the rank with the range's
// share won and matches, then under a hairline the last match: hero face,
// hero and length, the result, K/D/A and souls.
Item {
    id: view

    required property var dl
    readonly property var stats: dl.stats
    readonly property var match: dl.lastMatch
    readonly property var hero: dl.heroOf(match.hero)

    Row {
        id: upper
        width: view.width
        spacing: 14
        BoardDeadlockArt {
            anchors.verticalCenter: parent.verticalCenter
            width: 50
            height: 40
            cover: false
            source: view.dl.rankArt(view.dl.badge)
            alt: qsTr("%1 rank badge").arg(view.dl.rankName(view.dl.badge))
        }
        Column {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - 64
            Text {
                width: parent.width
                height: 20
                verticalAlignment: Text.AlignVCenter
                textFormat: Text.PlainText
                text: view.dl.rankName(view.dl.badge)
                elide: Text.ElideRight
                color: Theme.ink1
                font: view.dl.titleFont
                Accessible.role: Accessible.Heading
                Accessible.name: text
            }
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: qsTr("%1% won · %2 matches").arg(view.dl.winRate(view.stats.wins, view.stats.matches)).arg(view.stats.matches)
                elide: Text.ElideRight
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
    }

    Item {
        id: foot
        width: view.width
        height: 10 + 38
        y: Math.max(upper.height + 10, view.height - height)

        Rectangle {
            width: parent.width
            height: 1
            color: Theme.line
        }
        BoardDeadlockArt {
            id: face
            y: 10 + Math.round((38 - height) / 2)
            width: 28
            height: 28
            source: view.dl.heroArt(view.hero, "sm")
            initial: view.hero.name
        }
        Column {
            x: face.width + 10
            y: 10
            width: parent.width - x
            spacing: 2
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: qsTr("Last match · %1 · %2").arg(view.hero.name).arg(view.dl.clockText(view.match.duration))
                elide: Text.ElideRight
                color: Theme.ink3
                font: Theme.type.caption
            }
            Item {
                width: parent.width
                height: 20
                Row {
                    spacing: 8
                    BoardDeadlockChip {
                        full: true
                        win: view.match.win
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: view.match.k + "/" + view.match.d + "/" + view.match.a
                        color: Theme.ink2
                        font: Theme.type.numSm
                        Accessible.name: qsTr("%1 kills, %2 deaths, %3 assists").arg(view.match.k).arg(view.match.d).arg(view.match.a)
                    }
                }
                Text {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: qsTr("%1 souls").arg(view.dl.thousands(view.match.souls))
                    color: Theme.ink3
                    font: Theme.type.num
                }
            }
        }
    }
}
