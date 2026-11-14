import QtQuick
import com.yap.native
import Yap.Ui

// Lapsed, any size (Deadlock.tsx LapsedBody): no match in 30 days, so the
// tier badge dimmed, the rank and when it was last ranked, and the
// all-time numbers at the foot.
Item {
    id: view

    required property var dl
    readonly property var digest: dl.digest
    readonly property int badge: digest?.rank?.badge || digest?.previous_badge || 0
    readonly property string lastRanked: digest?.last_ranked_at
        ? Board.dayMonth(new Date(digest.last_ranked_at * 1000).toISOString()) : ""

    Row {
        id: upper
        width: view.width
        spacing: 14
        BoardDeadlockArt {
            anchors.verticalCenter: parent.verticalCenter
            width: 50
            height: 40
            opacity: 0.6
            cover: false
            source: view.dl.rankArt(Math.floor(view.badge / 10) * 10)
            alt: qsTr("%1 badge").arg(view.dl.tierName(view.badge))
        }
        Column {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - 64
            Text {
                width: parent.width
                height: 20
                verticalAlignment: Text.AlignVCenter
                textFormat: Text.PlainText
                text: view.dl.rankName(view.badge)
                elide: Text.ElideRight
                color: Theme.ink1
                font: view.dl.titleFont
                Accessible.role: Accessible.Heading
                Accessible.name: text
            }
            Text {
                visible: view.lastRanked.length > 0
                width: parent.width
                textFormat: Text.PlainText
                text: qsTr("Last ranked %1").arg(view.lastRanked)
                    + (view.digest?.season ? " · " + view.dl.seasonShort(view.digest.season.name) : "")
                elide: Text.ElideRight
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
    }

    Item {
        width: view.width
        height: 16
        y: Math.max(upper.height + 10, view.height - height)
        Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: qsTr("All time")
            color: Theme.ink3
            font: Theme.type.caption
        }
        Text {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: qsTr("%1 matches · %2% won").arg(view.digest?.all_time?.matches ?? 0)
                .arg(view.dl.winRate(view.digest?.all_time?.wins ?? 0, view.digest?.all_time?.matches ?? 0))
            color: Theme.ink2
            font: Theme.type.numSm
        }
    }
}
