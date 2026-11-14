pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// Placement, S (Deadlock.tsx PlacementBody): the unranked badge, matches
// left, one pip per placement match (solid for a win, ink-3 for a loss,
// outlined for one still to play) and last season's rank at the foot.
Item {
    id: view

    required property var dl
    readonly property int total: dl.digest?.season?.calibration_matches ?? 0
    readonly property int remaining: dl.digest?.rank?.calibration_left ?? 0
    readonly property int played: Math.max(0, total - remaining)
    readonly property double seasonStart: dl.digest?.season?.start ?? 0
    readonly property var results: dl.last10.filter(m => m.mode === "ranked" && m.at >= seasonStart).reverse()

    Row {
        id: upper
        width: view.width
        spacing: 14
        BoardDeadlockArt {
            anchors.verticalCenter: parent.verticalCenter
            width: 80
            height: 64
            cover: false
            source: view.dl.rankArt(0)
            alt: qsTr("Unranked badge")
        }
        Column {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - 94
            Text {
                width: parent.width
                height: 20
                verticalAlignment: Text.AlignVCenter
                textFormat: Text.PlainText
                text: qsTr("Placement")
                color: Theme.ink1
                font: view.dl.titleFont
                Accessible.role: Accessible.Heading
                Accessible.name: text
            }
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: qsTr("%1 of %2 matches left").arg(view.remaining).arg(view.total)
                elide: Text.ElideRight
                color: Theme.ink3
                font: Theme.type.caption
            }
            Item { width: 1; height: 8; visible: view.total > 0 }
            Row {
                visible: view.total > 0
                spacing: 3
                Accessible.ignored: true
                Repeater {
                    model: view.total
                    delegate: Rectangle {
                        required property int index
                        readonly property var result: index < view.played ? view.results[index] : undefined
                        width: 14
                        height: 6
                        radius: 1
                        color: index >= view.played ? "transparent" : result && !result.win ? Theme.ink3 : Theme.ink1
                        border.width: index >= view.played ? 1 : 0
                        border.color: Theme.lineStrong
                    }
                }
            }
        }
    }

    Row {
        visible: !!view.dl.digest?.previous_badge
        y: Math.max(upper.height + 10, view.height - height)
        height: 16
        spacing: 8
        Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: qsTr("Last season")
            color: Theme.ink3
            font: Theme.type.caption
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: view.dl.rankName(view.dl.digest?.previous_badge ?? 0)
            color: Theme.ink2
            font: Theme.type.numSm
        }
    }
}
