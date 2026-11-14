import QtQuick
import com.yap.native
import Yap.Ui

// Career, M (Deadlock.tsx CareerMedium): the rank badge and name with the
// season and this week's points, four numbers for the range (won, KDA,
// games, hours), the hero pool, and the last ten as letters at the foot.
Item {
    id: view

    required property var dl
    readonly property var stats: dl.stats
    readonly property var rank: dl.digest?.rank ?? null

    Column {
        id: upper
        width: view.width
        spacing: 10

        Row {
            width: parent.width
            spacing: 12
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
                width: parent.width - 62
                Text {
                    width: parent.width
                    height: 24
                    verticalAlignment: Text.AlignVCenter
                    textFormat: Text.PlainText
                    text: view.dl.placing ? qsTr("Placement") : view.dl.rankName(view.dl.badge)
                    elide: Text.ElideRight
                    color: Theme.ink1
                    font: Theme.type.sectionTitle
                    Accessible.role: Accessible.Heading
                    Accessible.name: text
                }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: (view.dl.digest?.season ? view.dl.seasonShort(view.dl.digest.season.name) : "Deadlock")
                        + (view.dl.placing ? " · " + qsTr("%1 placement matches left").arg(view.rank?.calibration_left)
                           : view.dl.weekDelta !== 0 ? " · " + qsTr("%1 this week").arg(view.dl.signed(view.dl.weekDelta)) : "")
                    elide: Text.ElideRight
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
        }

        Row {
            id: numbers
            width: parent.width
            spacing: 8
            readonly property real cell: (width - 24) / 4
            BoardDeadlockStat {
                dl: view.dl
                width: numbers.cell
                label: qsTr("Won")
                value: String(view.dl.winRate(view.stats.wins, view.stats.matches))
                unit: "%"
            }
            BoardDeadlockStat {
                dl: view.dl
                width: numbers.cell
                label: qsTr("KDA")
                value: view.dl.kda(view.stats).toFixed(1)
            }
            BoardDeadlockStat {
                dl: view.dl
                width: numbers.cell
                label: qsTr("Games")
                value: String(view.stats.matches)
            }
            BoardDeadlockStat {
                dl: view.dl
                width: numbers.cell
                label: qsTr("Hours")
                value: String(Math.round(view.stats.seconds / 3600))
            }
        }

        Column {
            visible: (view.stats.pool ?? []).length > 0
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
                range: view.dl.range
            }
        }
    }

    Column {
        id: foot
        visible: view.dl.last10.length > 0
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
}
