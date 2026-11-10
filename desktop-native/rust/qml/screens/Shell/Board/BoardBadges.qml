pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// Badge case (widgets/Badges.tsx): the owner's badges in the order they
// picked (or newest first), drawn by UserBadges. S is a row of marks with
// the latest one named under it; M and L name each one. Every badge is a
// key whose tip says what it is and when it was earned.
WidgetPlate {
    id: root

    required property var widget
    required property var boardData
    required property var ctx

    // lib/badges.ts BADGE_IDS: the readout counts against all of them.
    readonly property var knownIds: ["founder", "loud", "yap-team", "bug-hunter",
        "early-yapper", "server-owner", "voice-regular", "night-owl"]
    readonly property var earned: (root.boardData?.badges ?? []).filter(b => knownIds.indexOf(b.id) >= 0)
    // The store's view, already ordered for this widget.
    readonly property var shown: {
        const raw = root.boardData?.badges_view ?? ""
        if (!raw)
            return []
        try {
            const parsed = JSON.parse(raw)
            return Array.isArray(parsed) ? parsed : []
        } catch (e) {
            return []
        }
    }
    readonly property var awardedAt: {
        const map = {}
        for (const b of earned)
            map[b.id] = b.awarded_at ?? ""
        return map
    }
    readonly property var latest: {
        let best = null
        for (const b of earned)
            if (!best || String(b.awarded_at ?? "").localeCompare(String(best.awarded_at ?? "")) > 0)
                best = b
        return best
    }
    readonly property string latestName: {
        if (!latest)
            return ""
        const view = shown.find(v => v.id === latest.id)
        return view?.name ?? latest.id
    }
    readonly property real bodyWidth: root.body.width

    kind: widget.type
    size: widget.size
    draft: widget.draft ?? false
    readout: earned.length + " / " + knownIds.length

    function one(badge) {
        return JSON.stringify([badge])
    }

    WidgetEmpty {
        visible: root.shown.length === 0
        icon: "verified"
        title: qsTr("No badges yet")
        hint: root.size === "S" ? "" : qsTr("Badges come from voice time, servers and a few surprises.")
    }

    // S: a row of marks (six, then "+N"), the latest named at the foot.
    Item {
        visible: root.shown.length > 0 && root.size === "S"
        anchors.fill: parent

        UserBadges {
            width: parent.width
            badges: JSON.stringify(root.shown)
            size: "lg"
            maxVisible: 7
        }
        Text {
            visible: root.latest !== null
            anchors.bottom: parent.bottom
            width: parent.width
            textFormat: Text.PlainText
            text: qsTr("Latest: %1, %2").arg(root.latestName).arg(Board.dayMonth(root.latest?.awarded_at ?? ""))
            elide: Text.ElideRight
            color: Theme.ink3
            font: Theme.type.caption
        }
    }

    // M: two columns (one when narrow, then six at most), name and date.
    Grid {
        id: mGrid
        visible: root.shown.length > 0 && root.size === "M"
        width: parent.width
        readonly property bool single: root.bodyWidth < 240
        columns: single ? 1 : 2
        columnSpacing: 8
        rowSpacing: 8
        Accessible.role: Accessible.List
        Accessible.name: qsTr("Badges")

        Repeater {
            model: root.size === "M" ? root.shown.slice(0, mGrid.single ? 6 : 8) : []
            delegate: Row {
                id: mCell
                required property var modelData
                width: mGrid.single ? mGrid.width : Math.floor((mGrid.width - mGrid.columnSpacing) / 2)
                spacing: 10
                leftPadding: 4
                topPadding: 4
                bottomPadding: 4

                UserBadges {
                    width: 36
                    anchors.verticalCenter: parent.verticalCenter
                    badges: root.one(mCell.modelData)
                    size: "lg"
                }
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    width: mCell.width - 36 - mCell.spacing - mCell.leftPadding
                    spacing: 0
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: mCell.modelData.name ?? ""
                        elide: Text.ElideRight
                        color: Theme.ink1
                        font: Theme.type.occupant
                    }
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: Board.dayMonth(root.awardedAt[mCell.modelData.id] ?? "")
                        elide: Text.ElideRight
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                }
            }
        }
    }

    // L: eight across (four when narrow), each mark over its name.
    Grid {
        id: lGrid
        visible: root.shown.length > 0 && root.size === "L"
        width: parent.width
        readonly property bool compact: root.bodyWidth < 480
        columns: compact ? 4 : 8
        columnSpacing: 8
        rowSpacing: 8
        Accessible.role: Accessible.List
        Accessible.name: qsTr("Badges")

        Repeater {
            model: root.size === "L" ? root.shown.slice(0, lGrid.compact ? 4 : 8) : []
            delegate: Column {
                id: lCell
                required property var modelData
                width: Math.floor((lGrid.width - lGrid.columnSpacing * (lGrid.columns - 1)) / lGrid.columns)
                spacing: 8
                topPadding: 8
                bottomPadding: 8

                UserBadges {
                    width: 36
                    anchors.horizontalCenter: parent.horizontalCenter
                    badges: root.one(lCell.modelData)
                    size: "lg"
                }
                Text {
                    width: lCell.width - 8
                    anchors.horizontalCenter: parent.horizontalCenter
                    horizontalAlignment: Text.AlignHCenter
                    textFormat: Text.PlainText
                    text: lCell.modelData.name ?? ""
                    elide: Text.ElideRight
                    color: Theme.ink2
                    font: Theme.type.caption
                }
            }
        }
    }
}
