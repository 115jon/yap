import QtQuick
import com.yap.native
import Yap.Ui

// Live, S (and M without a known hero) (Deadlock.tsx LivePlate, compact):
// the hero's face (or the rank badge), what they are doing, the mode or
// rank, and the live clock. S keeps one footer line: the last match in the
// Hideout, else the party, else the 30-day count. M adds the 30-day hero
// pool and the last ten.
Item {
    id: view

    required property var dl
    readonly property bool medium: dl.size === "M"
    readonly property var hero: dl.liveHero
    readonly property var last: dl.lastMatch

    Column {
        id: upper
        width: view.width
        spacing: 10

        Row {
            width: parent.width
            spacing: 14
            Accessible.role: Accessible.StaticText
            Accessible.name: view.dl.liveTitle + ", " + view.dl.liveSubtitle

            Loader {
                anchors.verticalCenter: parent.verticalCenter
                sourceComponent: view.hero ? face : badge
                Component {
                    id: face
                    BoardDeadlockArt {
                        width: 56
                        height: 56
                        source: view.dl.heroArt(view.hero, "sm")
                        initial: view.hero?.name ?? ""
                        alt: view.hero?.name ?? ""
                    }
                }
                Component {
                    id: badge
                    BoardDeadlockArt {
                        width: 50
                        height: 40
                        cover: false
                        source: view.dl.rankArt(view.dl.badge)
                        alt: qsTr("%1 rank badge").arg(view.dl.rankName(view.dl.badge))
                    }
                }
            }
            Column {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - (view.hero ? 56 : 50) - 14
                Text {
                    width: parent.width
                    height: 20
                    verticalAlignment: Text.AlignVCenter
                    textFormat: Text.PlainText
                    text: view.dl.liveTitle
                    elide: Text.ElideRight
                    color: Theme.ink1
                    font: view.dl.titleFont
                }
                Text {
                    visible: text.length > 0
                    width: parent.width
                    textFormat: Text.PlainText
                    text: view.dl.liveSubtitle
                    elide: Text.ElideRight
                    color: Theme.ink3
                    font: Theme.type.caption
                }
                Item { width: 1; height: 6 }
                Row {
                    spacing: 6
                    Text {
                        id: clock
                        textFormat: Text.PlainText
                        text: view.dl.liveClock
                        color: Theme.ink1
                        font: view.dl.clockSmFont
                        Accessible.role: Accessible.StaticText
                        Accessible.name: text
                    }
                    Text {
                        visible: text.length > 0
                        anchors.baseline: clock.baseline
                        textFormat: Text.PlainText
                        text: view.dl.liveClockUnit
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                }
            }
        }

        Column {
            visible: view.medium && (view.dl.stats30.pool ?? []).length > 0
            width: parent.width
            spacing: 4
            BoardDeadlockZoneHead {
                width: parent.width
                label: qsTr("Hero pool")
                aside: qsTr("games · won")
            }
            BoardDeadlockPool {
                dl: view.dl
                width: parent.width
                range: "30d"
            }
        }
    }

    // M: the last ten at the foot.
    Column {
        visible: view.medium && view.dl.last10.length > 0
        width: view.width
        y: Math.max(upper.height + 10, view.height - height)
        spacing: 8
        BoardDeadlockZoneHead {
            width: parent.width
            label: qsTr("Last %1").arg(view.dl.last10.length)
            aside: view.dl.record(view.dl.last10)
        }
        BoardDeadlockLetters {
            dl: view.dl
            width: parent.width
        }
    }

    // S: one footer line.
    Item {
        visible: !view.medium
        width: view.width
        height: 20
        y: Math.max(upper.height + 10, view.height - height)

        Row {
            visible: view.dl.phase === "hideout" && view.last !== null
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: qsTr("Last match")
                color: Theme.ink3
                font: Theme.type.caption
            }
            BoardDeadlockChip {
                full: true
                win: view.last?.win ?? false
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: view.last ? view.last.k + "/" + view.last.d + "/" + view.last.a : ""
                color: Theme.ink2
                font: Theme.type.numSm
            }
        }
        BoardDeadlockParty {
            visible: !(view.dl.phase === "hideout" && view.last !== null) && view.dl.party.length > 0
            dl: view.dl
            width: parent.width
            anchors.verticalCenter: parent.verticalCenter
            text: view.dl.phase === "queue"
                ? qsTr("Party of %1 with %2").arg(view.dl.party.length + 1).arg(view.dl.joinNames(view.dl.party.map(m => m.name)))
                : ""
        }
        Row {
            visible: !(view.dl.phase === "hideout" && view.last !== null) && view.dl.party.length === 0
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: view.dl.stats30.matches
                color: Theme.ink1
                font: Theme.type.numSm
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: qsTr("matches in 30 days")
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
    }
}
