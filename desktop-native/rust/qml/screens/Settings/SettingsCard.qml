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
        width: parent.width
        height: 12 + 14
        Text {
            y: 12
            textFormat: Text.PlainText
            text: card.legend
            color: Theme.ink3
            font: Theme.type.silk
            Accessible.role: Accessible.Heading
        }
        Row {
            anchors.right: parent.right
            y: 12
            height: 14
            spacing: 12
            Loader {
                anchors.verticalCenter: parent.verticalCenter
                active: card.actions !== null
                sourceComponent: card.actions
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
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
        width: Math.min(parent.width, 560)
        textFormat: Text.PlainText
        text: card.desc
        wrapMode: Text.Wrap
        color: Theme.ink2
        font: Theme.type.description
        lineHeight: Theme.type.descriptionLine
        lineHeightMode: Text.FixedHeight
    }
    Item { width: 1; height: 12 }
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
