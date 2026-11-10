pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// Custom (widgets/Custom.tsx): the paid plate. The owner names the legend,
// writes a title and up to four short lines, and adds a picture. Without
// the perk only the owner sees it, tagged "Draft · only you" (`draft`).
WidgetPlate {
    id: root

    required property var widget
    required property var boardData
    required property var ctx

    readonly property string title: widget.cfg?.title ?? ""
    readonly property string image: widget.cfg?.asset ? (widget.cfg?.asset_url ?? "") : ""
    readonly property var filled: (widget.cfg?.lines ?? []).filter(line => String(line).trim().length > 0)
    readonly property bool empty: title.length === 0 && image.length === 0 && filled.length === 0
    readonly property bool slim: root.body.width < 200
    // S with a picture and no lines: the picture fills, the title under it.
    readonly property bool sPicture: size === "S" && image.length > 0 && filled.length === 0
    readonly property font titleFont: Qt.font({
        family: "Yap Bricolage Head",
        pixelSize: size === "S" ? 16 : 18,
        weight: Font.Bold,
        letterSpacing: -(size === "S" ? 16 : 18) * 0.01,
        hintingPreference: Theme.hinting
    })
    readonly property int titleLine: size === "S" ? 20 : 22

    kind: widget.type
    size: widget.size
    draft: widget.draft ?? false
    legend: widget.cfg?.legend ?? ""

    // The title (two lines) and the lines, each behind a short dash.
    component TextBlock: Column {
        id: block
        property int max: 4
        spacing: 6

        Text {
            visible: root.title.length > 0
            width: block.width
            textFormat: Text.PlainText
            text: root.title
            wrapMode: Text.Wrap
            maximumLineCount: 2
            elide: Text.ElideRight
            color: Theme.ink1
            font: root.titleFont
            lineHeight: root.titleLine
            lineHeightMode: Text.FixedHeight
            Accessible.role: Accessible.Heading
            Accessible.name: text
        }
        Column {
            visible: root.filled.length > 0
            width: block.width
            spacing: 4
            Repeater {
                model: root.filled.slice(0, block.max)
                delegate: Item {
                    id: lineItem
                    required property var modelData
                    width: block.width
                    height: lineText.height
                    Rectangle {
                        x: 0
                        y: Math.round(lineText.baselineOffset - 4)
                        width: 6
                        height: 1
                        color: Theme.ink3
                    }
                    Text {
                        id: lineText
                        x: 14
                        width: lineItem.width - x
                        textFormat: Text.PlainText
                        text: String(lineItem.modelData)
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                        color: Theme.ink2
                        font: Theme.type.description
                    }
                }
            }
        }
    }

    WidgetEmpty {
        visible: root.empty
        icon: "sparkle"
        title: root.ctx?.isOwner ? qsTr("Make it yours") : qsTr("Nothing here yet")
        hint: root.ctx?.isOwner ? qsTr("Add a title, a picture and up to four lines in Edit board.") : ""
    }

    // S, picture only: it fills, the title on one line under it.
    Item {
        visible: !root.empty && root.sPicture
        anchors.fill: parent
        WidgetImage {
            width: parent.width
            height: parent.height - (sTitle.visible ? sTitle.height + 10 : 0)
            source: root.sPicture ? root.image : ""
            decodeWidth: 640
        }
        Text {
            id: sTitle
            visible: root.title.length > 0
            anchors.bottom: parent.bottom
            width: parent.width
            textFormat: Text.PlainText
            text: root.title
            elide: Text.ElideRight
            color: Theme.ink1
            font: root.titleFont
            lineHeight: root.titleLine
            lineHeightMode: Text.FixedHeight
        }
    }

    // S with lines: a 56 px thumbnail (hidden when narrow) beside the text.
    Item {
        visible: !root.empty && root.size === "S" && !root.sPicture
        anchors.fill: parent
        WidgetImage {
            id: thumb
            visible: root.image.length > 0 && !root.slim
            width: 56
            height: 56
            source: visible ? root.image : ""
            decodeWidth: 160
        }
        TextBlock {
            x: thumb.visible ? thumb.width + 12 : 0
            width: parent.width - x
            max: 3
        }
    }

    // M: the picture across the top at 4:3, the text under it.
    Column {
        visible: !root.empty && root.size === "M"
        width: parent.width
        spacing: 10
        WidgetImage {
            visible: root.image.length > 0
            width: parent.width
            height: Math.round(width * 3 / 4)
            source: visible ? root.image : ""
            decodeWidth: 640
        }
        TextBlock {
            width: parent.width
            max: 4
        }
    }

    // L: the picture at full height (4:3, square when narrow) beside the text.
    Item {
        visible: !root.empty && root.size === "L"
        anchors.fill: parent
        WidgetImage {
            id: lImage
            visible: root.image.length > 0
            height: parent.height
            width: Math.round(root.body.width < 420 ? height : height * 4 / 3)
            source: visible ? root.image : ""
            decodeWidth: 480
        }
        TextBlock {
            x: lImage.visible ? lImage.width + 16 : 0
            width: parent.width - x
            max: 4
        }
    }
}
