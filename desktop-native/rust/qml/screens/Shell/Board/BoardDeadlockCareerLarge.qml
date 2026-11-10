import QtQuick
import com.yap.native
import Yap.Ui

// Career, L (Deadlock.tsx CareerLarge): three zones split by hairlines, the
// hero pool (dropped at 560 px and under), the last ten as faces, and the
// rank with this week's points over its 30-day history.
Item {
    id: view

    required property var dl
    readonly property bool wide: width > 560
    readonly property var stats: dl.stats

    Row {
        anchors.fill: parent
        spacing: 18

        Column {
            visible: view.wide
            width: 232
            spacing: 8
            BoardDeadlockZoneHead {
                width: parent.width
                label: qsTr("Hero pool")
                aside: view.stats.matches + " · " + view.dl.winRate(view.stats.wins, view.stats.matches) + "%"
            }
            BoardDeadlockPool {
                dl: view.dl
                width: parent.width
                range: view.dl.range
            }
        }
        Rectangle {
            visible: view.wide
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
        Rectangle {
            width: 1
            height: parent.height
            color: Theme.line
        }
        Column {
            width: Math.max(0, view.width - (view.wide ? 232 + 1 + 36 : 0) - recent.width - 1 - 36)
            spacing: 8
            Item {
                width: parent.width
                height: 28
                BoardDeadlockArt {
                    id: badgeArt
                    anchors.verticalCenter: parent.verticalCenter
                    width: 30
                    height: 24
                    cover: false
                    source: view.dl.rankArt(view.dl.badge)
                    alt: qsTr("%1 rank badge").arg(view.dl.rankName(view.dl.badge))
                }
                Text {
                    x: badgeArt.width + 8
                    width: parent.width - x - (delta.visible ? delta.implicitWidth + 8 : 0)
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: view.dl.badge ? view.dl.rankName(view.dl.badge) : qsTr("Placement")
                    elide: Text.ElideRight
                    color: Theme.ink1
                    font: view.dl.headFont
                    Accessible.role: Accessible.Heading
                    Accessible.name: text
                }
                Text {
                    id: delta
                    visible: view.dl.weekDelta !== 0
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: view.dl.signed(view.dl.weekDelta)
                    color: Theme.ink2
                    font: Theme.type.num
                }
            }
            BoardDeadlockRankHistory {
                dl: view.dl
                width: parent.width
                chartHeight: Math.max(40, Math.min(76, view.height - 28 - 8 - 4 - 14))
            }
        }
    }
}
