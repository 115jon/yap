pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// Live, M with a known hero (Deadlock.tsx LivePlate, M): the hero's art
// band with the mode tag (talk LED in a match) and the big live clock, the
// hero and what they are doing, the party, and at the foot today's results
// and the hero's last 30 days.
Item {
    id: view

    required property var dl
    readonly property var hero: dl.liveHero
    readonly property string phase: dl.phase
    readonly property bool roomy: width > 260

    Column {
        id: upper
        width: view.width
        spacing: 10

        BoardDeadlockBand {
            dl: view.dl
            hero: view.hero
            width: parent.width
            height: 150
            tag: view.dl.liveMode
            tagLed: view.phase === "match"
            clockLabel: view.phase === "match" ? qsTr("Match time") : view.phase === "queue" ? qsTr("In queue") : qsTr("In game")
            clock: view.dl.liveClock
        }
        Column {
            width: parent.width
            Accessible.role: Accessible.StaticText
            Accessible.name: heroName.text + ", " + doing.text + ", " + view.dl.liveClock
            Text {
                id: heroName
                width: parent.width
                height: 20
                verticalAlignment: Text.AlignVCenter
                textFormat: Text.PlainText
                text: view.hero?.name ?? ""
                elide: Text.ElideRight
                color: Theme.ink1
                font: view.dl.titleFont
            }
            Text {
                id: doing
                width: parent.width
                textFormat: Text.PlainText
                text: view.phase === "match" ? view.dl.liveSubtitle
                    : view.phase === "queue" ? qsTr("Looking for a match") : qsTr("In the Hideout")
                elide: Text.ElideRight
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
        BoardDeadlockParty {
            dl: view.dl
            width: parent.width
        }
    }

    Rectangle {
        y: foot.y
        width: view.width
        height: 1
        color: Theme.line
    }
    Column {
        id: foot
        width: view.width
        y: Math.max(upper.height + 10, view.height - height)
        spacing: 6
        topPadding: 11

        Item {
            visible: view.dl.today.length > 0
            width: parent.width
            height: 20
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: qsTr("Today")
                color: Theme.ink3
                font: Theme.type.caption
            }
            Row {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                spacing: 6
                Repeater {
                    model: view.roomy ? view.dl.today.slice(0, 4).reverse() : []
                    delegate: BoardDeadlockChip {
                        required property var modelData
                        full: true
                        win: modelData.win
                    }
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    leftPadding: 4
                    textFormat: Text.PlainText
                    text: view.dl.record(view.dl.today)
                    color: Theme.ink2
                    font: Theme.type.numSm
                }
            }
        }
        Item {
            width: parent.width
            height: 16
            Text {
                visible: view.roomy || !view.dl.liveHeroStats
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, parent.width - value.implicitWidth - 8)
                textFormat: Text.PlainText
                text: view.dl.liveHeroStats ? qsTr("%1, 30 days").arg(view.hero?.name ?? "") : qsTr("30 days")
                elide: Text.ElideRight
                color: Theme.ink3
                font: Theme.type.caption
            }
            Text {
                id: value
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: view.dl.liveHeroStats
                    ? qsTr("%1 matches · %2% won").arg(view.dl.liveHeroStats.matches)
                        .arg(view.dl.winRate(view.dl.liveHeroStats.wins, view.dl.liveHeroStats.matches))
                    : qsTr("%1 matches").arg(view.dl.stats30.matches)
                color: Theme.ink2
                font: Theme.type.numSm
            }
        }
    }
}
