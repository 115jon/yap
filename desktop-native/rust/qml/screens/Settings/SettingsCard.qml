import QtQuick
import com.yap.native

// A C2 rule card (primitives.tsx SettingsCard, ui/plate.tsx RuleCard): no
// box, a 2 px ink-1 rule, the section's silkscreen legend and its
// "02 / 05" on the rule line, an optional description, then its rows with
// a hairline between them. A schema section (`sectionId`) hides when the
// search leaves none of its rows; `title` makes a card outside the map.
Column {
    id: card

    property string sectionId
    property string title
    property string description
    // Trailing controls on the legend line, before the index.
    property Component actions: null
    property Component legendLead: null
    default property alias rows: list.data

    readonly property var section: SettingsNav.sections[sectionId] ?? null
    readonly property string legend: title.length > 0 ? title : section?.title ?? ""
    readonly property string desc: description.length > 0 ? description : section?.desc ?? ""
    readonly property string index: section ? Settings.sectionIndex(sectionId) : ""

    // Only the last shown row loses its hairline.
    function updateRules() {
        let last = null
        for (const row of list.children) {
            if (row.rule !== undefined) {
                row.rule = true
                if (row.visible)
                    last = row
            }
        }
        if (last)
            last.rule = false
    }

    visible: title.length > 0 || SettingsNav.sectionShown(sectionId)
    spacing: 0
    Accessible.role: Accessible.Grouping
    Accessible.name: legend

    Rectangle {
        width: parent.width
        height: 2
        color: Theme.ink1
    }
    Item {
        id: legendLine
        // The line is as tall as its trailing control (min-h 14), and the
        // legend sits on the control's baseline (items-baseline): a 40 px
        // key puts it 16 down, a 32 px one 12.
        readonly property real controlHeight: actionsLoader.item && actionsLoader.item.visible ? actionsLoader.height : 0
        readonly property real leadHeight: leadLoader.item ? leadLoader.height : 0
        readonly property real lift: controlHeight > 14 ? Math.round((controlHeight - 14) / 2) + 3 : 0
        width: parent.width
        height: 12 + Math.max(14, controlHeight, leadHeight)
        Row {
            y: 12 + legendLine.lift
            spacing: 8
            // A mark before the legend (the override editor's role dot);
            // the line grows to hold it.
            Loader {
                id: leadLoader
                anchors.verticalCenter: parent.verticalCenter
                active: card.legendLead !== null
                visible: active
                sourceComponent: card.legendLead
            }
            Text {
                textFormat: Text.PlainText
                text: card.legend
                color: Theme.ink3
                font: Theme.type.silk
                Accessible.role: Accessible.Heading
            }
        }
        Row {
            anchors.right: parent.right
            y: 12
            spacing: 12
            Loader {
                id: actionsLoader
                active: card.actions !== null
                sourceComponent: card.actions
            }
            Text {
                y: legendLine.lift
                textFormat: Text.PlainText
                text: card.index
                color: Theme.ink3
                font: Theme.type.silk
            }
        }
    }
    Text {
        visible: card.desc.length > 0
        topPadding: 12
        // max-w-[62ch] of 13 px Geist.
        width: Math.min(parent.width, 534)
        textFormat: Text.PlainText
        text: card.desc
        wrapMode: Text.Wrap
        color: Theme.ink2
        font: Theme.type.description
        lineHeight: Theme.type.descriptionLine
        lineHeightMode: Text.FixedHeight
    }
    // RuleCard's mt-3 under the legend; a description sits flush on the
    // rows (their own py-4 spaces them).
    Item { width: 1; height: card.desc.length > 0 ? 0 : 12 }
    Column {
        id: list
        width: parent.width
        onChildrenChanged: Qt.callLater(card.updateRules)
    }
    Connections {
        target: SettingsNav
        function onTrimmedChanged() { Qt.callLater(card.updateRules) }
    }
}
