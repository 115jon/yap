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
    property color ground: Theme.ply0
    // Set by the card: the last shown row has no hairline.
    property bool rule: true
    default property alias content: controls.data
    property alias footer: footerSlot.data

    readonly property string shownLabel: label.length > 0 ? label : SettingsNav.label(rowId)
    readonly property string shownDesc: description.length > 0 ? description : custom ? "" : SettingsNav.desc(rowId)
    readonly property bool field: layout === "field" && !hideLabel
    readonly property bool side: !field && (layout === "inline" || (layout === "stack" && width >= 560))
    readonly property bool textShown: !hideLabel && !field
    readonly property bool highlighted: !custom && SettingsNav.trimmed.length > 0
    readonly property bool landed: !custom && SettingsNav.target === rowId && rowId.length > 0

    width: parent ? parent.width : 0
    visible: custom || SettingsNav.matches(rowId)
    implicitHeight: 16 + body.height + footerSlot.implicitHeight + 16
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
        y: 16
        width: parent.width
        height: row.field ? 8 + controlSlot.height + (fieldDesc.visible ? 8 + fieldDesc.implicitHeight : 0)
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
                Text {
                    visible: row.shownDesc.length > 0
                    width: Math.min(parent.width, 520)
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
                text: row.shownLabel
                color: controlSlot.activeFocus ? Theme.ink1 : Theme.ink2
                font: Theme.type.silk
            }
        }

        FocusScope {
            id: controlSlot
            x: row.side ? parent.width - width : 0
            y: row.field ? 8 : row.side ? (parent.height - height) / 2 : row.textShown ? textBlock.height + 12 : 0
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
            width: Math.min(parent.width, 520)
            text: row.shownDesc
            wrapMode: Text.Wrap
            color: Theme.ink2
            font: Theme.type.description
        }
    }

    Column {
        id: footerSlot
        y: body.y + body.height
        width: parent.width
    }

    Rectangle {
        visible: row.rule
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.line
    }
}
