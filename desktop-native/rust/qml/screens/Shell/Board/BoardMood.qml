import QtQuick
import com.yap.native

// Mood (widgets/Mood.tsx): Yappy in a pose (the still bark frame or a
// YappyPose) beside one line about how it's going, and when that line
// clears. A cleared mood keeps Yappy, quietly. Nothing moves.
WidgetPlate {
    id: root

    required property var widget
    required property var boardData
    required property var ctx

    readonly property string pose: widget.cfg?.pose ?? "bark"
    readonly property string moodText: widget.cfg?.text ?? ""
    readonly property real now: ctx?.at ?? Date.now()
    readonly property real clearsAt: widget.cfg?.clears_at ? Date.parse(widget.cfg.clears_at) : NaN
    readonly property bool cleared: moodText.length === 0 || (isFinite(clearsAt) && clearsAt <= now)
    readonly property bool big: size === "M"
    readonly property bool slim: root.body.width < 200
    // Stacked: Yappy over the words (M, or a narrow S).
    readonly property bool stacked: big || slim
    // The bark's grid is 32 cells, YappyPose's "stuffed" 40: match widths.
    readonly property int cell: big ? 3 : 2
    readonly property real yappyScale: !big && slim ? 0.75 : 1

    kind: widget.type
    size: widget.size
    draft: widget.draft ?? false

    function clearsLabel() {
        const date = new Date(clearsAt)
        if (clearsAt - now <= 86400000 && date.getHours() === 0 && date.getMinutes() === 0)
            return qsTr("at midnight")
        if (date.toDateString() === new Date(now).toDateString())
            return qsTr("at %1").arg(Qt.formatTime(date, "hh:mm"))
        return Board.dateTime(date.toISOString())
    }

    Item {
        id: yappy
        readonly property real w: root.pose === "bark"
            ? (!root.big && root.slim ? 48 : 32 * root.cell)
            : pose.cols * root.cell * (root.pose === "stuffed" ? 32 / 40 : 1) * root.yappyScale
        readonly property real h: root.pose === "bark"
            ? w
            : 34 * root.cell * (root.pose === "stuffed" ? 32 / 40 : 1) * root.yappyScale
        x: 0
        y: root.stacked ? Math.max(0, Math.round((parent.height - content.height) / 2))
            : Math.max(0, Math.round((parent.height - h) / 2))
        width: w
        height: h
        opacity: root.cleared ? 0.6 : 1

        YapBark {
            visible: root.pose === "bark"
            width: yappy.w
            height: yappy.h
            cell: root.cell
            barking: false
        }
        Item {
            visible: root.pose !== "bark"
            anchors.fill: parent
            YappyPose {
                id: pose
                pose: root.pose === "bark" ? "sad" : root.pose
                cell: root.cell
                width: yappy.w
                height: yappy.h
            }
        }
    }

    Column {
        id: words
        x: root.stacked ? 0 : yappy.width + 14
        y: root.stacked ? yappy.y + yappy.height + (root.big ? 16 : 8)
            : Math.max(0, Math.round((parent.height - height) / 2))
        width: parent.width - x
        spacing: 4
        opacity: root.cleared ? 0.6 : 1

        Text {
            visible: root.cleared
            width: parent.width
            textFormat: Text.PlainText
            text: root.ctx?.isOwner ? qsTr("No mood set") : qsTr("No mood right now")
            wrapMode: Text.Wrap
            maximumLineCount: 2
            elide: Text.ElideRight
            color: Theme.ink2
            font: Theme.type.description
        }
        Text {
            visible: !root.cleared
            width: parent.width
            textFormat: Text.PlainText
            text: root.moodText
            wrapMode: Text.Wrap
            maximumLineCount: root.big ? 6 : 3
            elide: Text.ElideRight
            color: Theme.ink1
            font: Qt.font({
                family: "Yap Bricolage Head",
                pixelSize: root.big ? 22 : root.slim ? 15 : 17,
                weight: Font.Bold,
                letterSpacing: -(root.big ? 22 : root.slim ? 15 : 17) * 0.01,
                hintingPreference: Theme.hinting
            })
            lineHeight: root.big ? 26 : root.slim ? 19 : 21
            lineHeightMode: Text.FixedHeight
        }
        Text {
            visible: !root.cleared && isFinite(root.clearsAt)
            width: parent.width
            textFormat: Text.PlainText
            text: visible ? qsTr("Clears %1").arg(root.clearsLabel()) : ""
            elide: Text.ElideRight
            color: Theme.ink3
            font: Theme.type.caption
        }
    }

    // The stacked block's height, for centring it in the body.
    Item {
        id: content
        visible: false
        height: yappy.h + (root.big ? 16 : 8) + words.implicitHeight
    }
}
