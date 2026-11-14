import QtQuick
import com.yap.native
import Yap.Ui

// Career, S (Deadlock.tsx CareerSmall): the big rank badge, its name and
// the share won big; the range's matches and the main hero at the foot.
Item {
    id: view

    required property var dl
    readonly property var stats: dl.stats
    readonly property var main: (stats.pool ?? []).length > 0 ? dl.heroOf(stats.pool[0].hero) : null

    Row {
        id: upper
        width: view.width
        spacing: 14
        BoardDeadlockArt {
            anchors.verticalCenter: parent.verticalCenter
            width: 80
            height: 64
            cover: false
            source: view.dl.rankArt(view.dl.badge)
            alt: qsTr("%1 rank badge").arg(view.dl.rankName(view.dl.badge))
        }
        Column {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - 94
            spacing: 4
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
            Row {
                spacing: 1
                Accessible.role: Accessible.StaticText
                Accessible.name: qsTr("%1% won").arg(view.dl.winRate(view.stats.wins, view.stats.matches))
                Text {
                    id: big
                    textFormat: Text.PlainText
                    text: view.dl.winRate(view.stats.wins, view.stats.matches)
                    color: Theme.ink1
                    font: view.dl.bigFont
                }
                Text {
                    anchors.baseline: big.baseline
                    textFormat: Text.PlainText
                    text: "%"
                    color: Theme.ink3
                    font: Qt.font({ family: "Yap Martian Num", pixelSize: 13, weight: Font.Medium })
                }
                Text {
                    anchors.baseline: big.baseline
                    leftPadding: 5
                    textFormat: Text.PlainText
                    text: qsTr("won")
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
        }
    }

    Item {
        width: view.width
        height: 20
        y: Math.max(upper.height + 10, view.height - height)
        Row {
            id: count
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            Text {
                textFormat: Text.PlainText
                text: view.stats.matches
                color: Theme.ink1
                font: Theme.type.numSm
            }
            Text {
                textFormat: Text.PlainText
                text: qsTr("matches")
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
        Row {
            visible: view.main !== null
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            BoardDeadlockArt {
                width: 20
                height: 20
                source: view.dl.heroArt(view.main, "sm")
                initial: view.main?.name ?? ""
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, view.width - count.width - 8 - 28 - 8)
                textFormat: Text.PlainText
                text: qsTr("%1 main").arg(view.main?.name ?? "")
                elide: Text.ElideRight
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
    }
}
