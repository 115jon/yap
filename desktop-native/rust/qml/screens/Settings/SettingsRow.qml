import QtQuick
import com.yap.native

// One settings row (primitives.tsx SettingsRow): the label (Geist 15/500)
// over its description (13, ink-2) on the leading side, the control on the
// trailing side, 16 px above and below and a hairline under it.
//   inline  the control always trails
//   stack   the control drops under the text below 560 px
//   block   the control always sits under the text, full width
//   field   the label is printed across the control's top edge (a legend
//           plate) and the description goes under it
//   label   server settings' FieldRow (server-settings/Field.tsx): px-4
//           py-3, the label at 13/500 over the control, 6 apart, and the
//           helper at 12 in ink-3 under it
// Labels and descriptions come from the map by `rowId`; `custom` rows
// (outside it) pass their own and ignore the search. A row a search hit or
// a deep link lands on flashes the accent tint.
Item {
    id: row

    property string rowId
    property string layout: "stack"
    property string label
    property string description
    property bool disabled: false
    property bool hideLabel: false
    property bool custom: false
    property Component leading: null
    // Drawn in place of the description text (the profile editor's name
    // in its style).
    property Component descriptionItem: null
    property color ground: Theme.ply0
    // Set by the card: the last shown row has no hairline.
    property bool rule: true
    default property alias content: controls.data
    property alias footer: footerSlot.data

    readonly property string shownLabel: label.length > 0 ? label : SettingsNav.label(rowId)
    readonly property string shownDesc: description.length > 0 ? description : custom ? "" : SettingsNav.desc(rowId)
    readonly property bool field: layout === "field" && !hideLabel
    readonly property bool labeled: layout === "label"
    // `stack` reads the settings pane's width (@container in
    // SettingsWorkbench.tsx), which is the row's own unless a split page
    // narrows the row (the override editor).
    property real containerWidth: width
    readonly property bool side: !field && !labeled && (layout === "inline" || (layout === "stack" && containerWidth >= 560))
    readonly property bool textShown: !hideLabel && !field && !labeled
    readonly property bool highlighted: !custom && SettingsNav.trimmed.length > 0
    readonly property bool landed: !custom && SettingsNav.target === rowId && rowId.length > 0

    width: parent ? parent.width : 0
    visible: custom || SettingsNav.matches(rowId)
    // The hairline is the row's border-b, under its padding.
    // A `label` row is server-settings FieldRow: no border of its own.
    readonly property bool ruled: rule && !labeled
    implicitHeight: (labeled ? 24 : 32) + body.height + footerSlot.implicitHeight + (ruled ? 1 : 0)
    onVisibleChanged: Qt.callLater(() => parent?.parent?.updateRules?.())

    Accessible.role: Accessible.Grouping
    Accessible.name: shownLabel
    Accessible.description: shownDesc

    // The landing tint bleeds 12 px past the column; the hairline does not.
    Rectangle {
        id: landing
        x: -12
        width: parent.width + 24
        height: parent.height
        radius: Theme.radius
        color: Theme.accentTint
        opacity: 0
        states: State {
            name: "landed"
            when: row.landed
            PropertyChanges { landing.opacity: 1 }
        }
        transitions: Transition {
            from: "landed"
            OpacityAnimator { duration: 1200; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
        }
    }

    Item {
        id: body
        x: row.labeled ? 16 : 0
        y: row.labeled ? 12 : 16
        width: parent.width - 2 * x
        height: row.labeled ? 18 + 6 + controlSlot.height + (labeledDesc.visible ? 6 + labeledDesc.implicitHeight : 0)
            : row.field ? 8 + controlSlot.height + (fieldDesc.visible ? 8 + fieldDesc.implicitHeight : 0)
            : row.side ? Math.max(textBlock.height, controlSlot.height)
            : (row.textShown ? textBlock.height : 0) + (row.textShown && controlSlot.height > 0 ? 12 : 0) + controlSlot.height

        Row {
            id: textBlock
            visible: row.textShown
            width: row.side ? parent.width - controlSlot.width - 16 : parent.width
            y: row.side ? (parent.height - height) / 2 : 0
            spacing: 12
            Loader {
                id: lead
                anchors.verticalCenter: parent.verticalCenter
                active: row.leading !== null
                visible: active
                sourceComponent: row.leading
            }
            Column {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - (lead.visible ? lead.width + 12 : 0)
                spacing: 2
                Text {
                    width: parent.width
                    text: row.highlighted ? SettingsNav.highlight(row.shownLabel) : row.shownLabel
                    textFormat: row.highlighted ? Text.RichText : Text.PlainText
                    wrapMode: Text.Wrap
                    color: row.disabled ? Theme.ink2 : Theme.ink1
                    font: Theme.type.setting
                    lineHeight: 20
                    lineHeightMode: Text.FixedHeight
                }
                Loader {
                    active: row.descriptionItem !== null
                    visible: active
                    width: Math.min(parent.width, 517)
                    sourceComponent: row.descriptionItem
                }
                Text {
                    visible: row.shownDesc.length > 0 && row.descriptionItem === null
                    width: Math.min(parent.width, 517) // max-w-[60ch]
                    text: row.highlighted ? SettingsNav.highlight(row.shownDesc) : row.shownDesc
                    textFormat: row.highlighted ? Text.RichText : Text.PlainText
                    wrapMode: Text.Wrap
                    color: Theme.ink2
                    font: Theme.type.description
                    lineHeight: Theme.type.descriptionLine
                    lineHeightMode: Text.FixedHeight
                }
            }
        }

        // A field row's legend, across the control's top edge.
        Rectangle {
            visible: row.field
            x: 12
            z: 1
            width: legend.implicitWidth + 8
            height: 14
            color: row.ground
            Text {
                id: legend
                x: 4
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: row.shownLabel
                color: controlSlot.activeFocus ? Theme.ink1 : Theme.ink2
                font: Theme.type.silk
            }
        }

        Text {
            visible: row.labeled
            width: parent.width
            textFormat: Text.PlainText
            text: row.shownLabel
            elide: Text.ElideRight
            color: Theme.ink1
            font.family: "Yap Geist Medium"
            font.pixelSize: 13
            font.weight: Font.Medium
            lineHeight: Theme.type.descriptionLine
            lineHeightMode: Text.FixedHeight
        }

        FocusScope {
            id: controlSlot
            x: row.side ? parent.width - width : 0
            y: row.labeled ? 18 + 6 : row.field ? 8 : row.side ? (parent.height - height) / 2 : row.textShown ? textBlock.height + 12 : 0
            width: row.side ? controls.implicitWidth : parent.width
            height: controls.implicitHeight
            enabled: !row.disabled
            // Sizes from its shown children only.
            Row {
                id: controls
                width: parent.width
                spacing: 8
            }
        }

        Text {
            id: fieldDesc
            visible: row.field && row.shownDesc.length > 0
            y: controlSlot.y + controlSlot.height + 8
            width: Math.min(parent.width, 517) // max-w-[60ch]
            textFormat: Text.PlainText
            text: row.shownDesc
            wrapMode: Text.Wrap
            color: Theme.ink2
            font: Theme.type.description
        }
    }

    Text {
        id: labeledDesc
        visible: row.labeled && row.shownDesc.length > 0
        x: body.x
        y: body.y + controlSlot.y + controlSlot.height + 6
        width: body.width
        textFormat: Text.PlainText
        text: row.shownDesc
        wrapMode: Text.Wrap
        color: Theme.ink3
        font: Theme.type.caption
        lineHeight: Theme.type.captionLine
        lineHeightMode: Text.FixedHeight
    }

    Column {
        id: footerSlot
        x: body.x
        y: body.y + body.height
        width: body.width
    }

    Rectangle {
        visible: row.ruled
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.line
    }
}
