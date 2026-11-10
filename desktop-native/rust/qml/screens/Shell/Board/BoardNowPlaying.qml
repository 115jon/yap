import QtQuick
import com.yap.native

// Now playing (widgets/NowPlaying.tsx): the owner's live game from
// presence (ctx.game), else the last finished game when the owner chose
// "last played". Play time this week and all time come from
// data.game_totals, keyed by the lower-cased game name. A live session
// clock ticks once a second, only while the plate is visible.
WidgetPlate {
    id: root

    required property var widget
    required property var boardData
    required property var ctx

    kind: widget.type
    size: widget.size
    draft: widget.draft

    readonly property var shown: {
        const game = root.ctx?.game ?? null
        if (game && game.name) {
            return {
                live: true,
                name: String(game.name),
                art: game.art ?? "",
                line: game.line ?? "",
                startedAt: game.startedAt ?? root.ctx?.at ?? Date.now(),
                endedAt: "",
                sessionSeconds: 0
            }
        }
        const last = root.boardData?.last_game ?? null
        if ((root.widget.cfg?.when_idle ?? "last_played") === "last_played" && last && last.name) {
            return {
                live: false,
                name: String(last.name),
                art: last.icon_url ?? "",
                line: "",
                startedAt: 0,
                endedAt: last.ended_at ?? "",
                sessionSeconds: last.seconds ?? 0
            }
        }
        return null
    }
    readonly property var totals: shown ? (root.boardData?.game_totals?.[shown.name.toLowerCase()] ?? null) : null
    readonly property real bw: body.width
    readonly property real bh: body.height

    // The live clock's "now", ticked only while visible.
    property double now: Date.now()
    Timer {
        interval: 1000
        repeat: true
        running: root.visible && (root.shown?.live ?? false)
        triggeredOnStart: true
        onTriggered: root.now = Date.now()
    }

    readout: shown?.live ? qsTr("Playing") : (shown?.endedAt ? qsTr("Last played") : "")
    readoutLed: shown?.live ? "talk" : ""
    readoutLive: shown?.live ?? false

    readonly property string sessionValue: !shown ? ""
        : shown.live ? GameClock.elapsed(root.now - shown.startedAt)
        : shown.sessionSeconds > 0 ? Board.duration(shown.sessionSeconds) : ""
    readonly property string sessionLabel: !shown ? ""
        : shown.live ? qsTr("this session")
        : shown.endedAt ? Board.ago(shown.endedAt) : ""

    readonly property font titleFont: Qt.font({ family: "Yap Bricolage Head", pixelSize: size === "L" ? 20 : 16, weight: Font.Bold, letterSpacing: size === "L" ? -0.2 : -0.16 })

    WidgetEmpty {
        visible: !root.shown
        icon: "gamepad"
        title: root.ctx?.isOwner ? qsTr("Nothing played yet") : qsTr("Not in a game right now")
        hint: root.ctx?.isOwner ? qsTr("Games show up here while you play, if you share your activity.") : ""
    }

    // One art box, one title block and one stats row, placed per size.
    // S: art beside the name (stacked under 200 px), stats at the foot.
    // M: a 16:9 art band, the name under it, stats at the foot.
    // L: art the body's height (square under 420 px), name and stats beside.
    readonly property bool stackS: size === "S" && bw <= 200
    readonly property real artW: size === "S" ? (stackS ? 48 : 56)
        : size === "M" ? bw
        : (bw <= 420 ? bh : Math.round(bh * 16 / 9))
    readonly property real artH: size === "S" ? artW : size === "M" ? Math.round(bw * 9 / 16) : bh
    readonly property real textX: size === "S" ? (stackS ? 0 : artW + 12) : size === "M" ? 0 : artW + 16
    readonly property real textY: size === "S" ? (stackS ? artH + 8 : 0) : size === "M" ? artH + 10 : 0

    Item {
        id: art
        visible: root.shown !== null
        width: root.artW
        height: root.artH
        WidgetImage {
            anchors.fill: parent
            visible: (root.shown?.art ?? "").length > 0
            source: visible ? root.shown.art : ""
            alt: ""
            decodeWidth: Math.max(64, Math.round(art.width * 2))
            // Game icons are 64 px (the catalog's only size): no stretching.
            upscale: false
        }
        Rectangle {
            anchors.fill: parent
            visible: (root.shown?.art ?? "").length === 0
            radius: Theme.radius
            color: Theme.ply4
            Text {
                anchors.centerIn: parent
                textFormat: Text.PlainText
                text: (root.shown?.name ?? "").slice(0, 1).toUpperCase()
                color: Theme.ink3
                font: Qt.font({ family: "Yap Bricolage Head ExtraBold", pixelSize: 22, weight: Font.ExtraBold })
            }
        }
    }

    Column {
        visible: root.shown !== null
        x: root.textX
        y: root.textY
        width: Math.max(0, root.bw - x)
        spacing: 2
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: root.shown?.name ?? ""
            elide: Text.ElideRight
            maximumLineCount: 1
            color: Theme.ink1
            font: root.titleFont
            lineHeight: root.size === "L" ? 24 : 20
            lineHeightMode: Text.FixedHeight
            Accessible.role: Accessible.StaticText
            Accessible.name: text
        }
        Text {
            width: parent.width
            visible: (root.shown?.line ?? "").length > 0
            textFormat: Text.PlainText
            text: root.shown?.line ?? ""
            elide: Text.ElideRight
            maximumLineCount: 1
            color: Theme.ink3
            font: Theme.type.caption
        }
    }

    Flow {
        visible: root.shown !== null
        x: root.size === "L" ? root.textX : 0
        width: Math.max(0, root.bw - x)
        y: root.bh - height
        spacing: 16
        Repeater {
            model: [
                { value: root.sessionValue, label: root.sessionLabel, show: root.sessionValue.length > 0 },
                { value: root.totals ? Board.duration(root.totals.week ?? 0) : "", label: qsTr("this week"), show: root.totals !== null && root.bw > 240 },
                { value: root.totals ? Board.hours(root.totals.all ?? 0) : "", label: qsTr("all time"), show: root.totals !== null && root.size === "L" }
            ]
            delegate: Row {
                required property var modelData
                visible: modelData.show
                spacing: 6
                Accessible.role: Accessible.StaticText
                Accessible.name: modelData.value + " " + modelData.label
                Text {
                    textFormat: Text.PlainText
                    text: parent.modelData.value
                    color: Theme.ink1
                    font: Theme.type.num
                    lineHeight: 16
                    lineHeightMode: Text.FixedHeight
                }
                Text {
                    textFormat: Text.PlainText
                    text: parent.modelData.label
                    color: Theme.ink3
                    font: Theme.type.caption
                    lineHeight: 16
                    lineHeightMode: Text.FixedHeight
                }
            }
        }
    }
}
