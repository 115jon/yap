import QtQuick
import com.yap.native
import Yap.Ui

// Live, L (Deadlock.tsx LivePlate, L): the hero's art band with the clock
// on the left (dropped at 560 px and under, or with no known hero), what
// they are doing with the party or the mode, the rank with today's record
// and the hero's 30 days; the career strip stays on the right so the plate
// keeps its shape.
Item {
    id: view

    required property var dl
    readonly property var hero: dl.liveHero
    readonly property bool showBand: hero !== null && width > 560

    Row {
        anchors.fill: parent
        spacing: 18

        BoardDeadlockBand {
            visible: view.showBand
            dl: view.dl
            hero: view.hero
            width: 230
            height: Math.min(134, view.height)
            tag: view.dl.liveMode
            tagLed: view.dl.phase === "match"
            clockLabel: qsTr("As %1").arg(view.hero?.name ?? "")
            clock: view.dl.liveClock
        }

        Item {
            id: middle
            width: Math.max(0, view.width - (view.showBand ? 230 + 18 : 0) - 18 - 1 - 18 - recent.width)
            height: parent.height

            Column {
                width: parent.width
                spacing: 4
                Accessible.role: Accessible.StaticText
                Accessible.name: view.dl.liveTitle + ", " + view.dl.liveSubtitle + ", " + view.dl.liveClock
                Text {
                    width: parent.width
                    height: 22
                    verticalAlignment: Text.AlignVCenter
                    textFormat: Text.PlainText
                    text: view.dl.liveTitle
                    elide: Text.ElideRight
                    color: Theme.ink1
                    font: Theme.type.columnTitle
                }
                BoardDeadlockParty {
                    dl: view.dl
                    width: parent.width
                }
                Text {
                    visible: view.dl.party.length === 0
                    width: parent.width
                    textFormat: Text.PlainText
                    text: view.dl.liveSubtitle + (view.hero ? "" : " · " + view.dl.liveClock)
                    elide: Text.ElideRight
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }

            Column {
                id: foot
                width: parent.width
                anchors.bottom: parent.bottom
                spacing: 4

                Rectangle {
                    width: parent.width
                    height: 1
                    color: Theme.line
                }
                Item {
                    width: parent.width
                    height: 6 + 24
                    BoardDeadlockArt {
                        id: badgeArt
                        y: 6
                        width: 30
                        height: 24
                        cover: false
                        source: view.dl.rankArt(view.dl.badge)
                        alt: qsTr("%1 rank badge").arg(view.dl.rankName(view.dl.badge))
                    }
                    Text {
                        x: badgeArt.width + 8
                        y: 6
                        height: 24
                        width: parent.width - x - (today.visible ? today.implicitWidth + 8 : 0)
                        verticalAlignment: Text.AlignVCenter
                        textFormat: Text.PlainText
                        text: view.dl.badge ? view.dl.rankName(view.dl.badge) : qsTr("Placement")
                        elide: Text.ElideRight
                        color: Theme.ink1
                        font: view.dl.headFont
                    }
                    Row {
                        id: today
                        visible: view.dl.today.length > 0
                        anchors.right: parent.right
                        y: 6
                        height: 24
                        spacing: 8
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: qsTr("Today")
                            color: Theme.ink3
                            font: Theme.type.caption
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: view.dl.record(view.dl.today)
                            color: Theme.ink2
                            font: Theme.type.numSm
                        }
                    }
                }
                Item {
                    visible: !!view.dl.liveHeroStats && !!view.hero
                    width: parent.width
                    height: 16
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        width: Math.min(implicitWidth, parent.width - heroValue.implicitWidth - 8)
                        textFormat: Text.PlainText
                        text: qsTr("%1, 30 days").arg(view.hero?.name ?? "")
                        elide: Text.ElideRight
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                    Text {
                        id: heroValue
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: view.dl.liveHeroStats
                            ? qsTr("%1 · %2% won").arg(view.dl.liveHeroStats.matches)
                                .arg(view.dl.winRate(view.dl.liveHeroStats.wins, view.dl.liveHeroStats.matches))
                            : ""
                        color: Theme.ink2
                        font: Theme.type.numSm
                    }
                }
            }
        }

        Rectangle {
            width: 1
            height: parent.height
            color: Theme.line
        }

        Column {
            id: recent
            width: strip.width
            spacing: 8
            BoardDeadlockZoneHead {
                width: parent.width
                label: qsTr("Last %1").arg(view.dl.last10.length)
                aside: view.dl.record(view.dl.last10)
            }
            BoardDeadlockStrip {
                id: strip
                dl: view.dl
            }
        }
    }
}
