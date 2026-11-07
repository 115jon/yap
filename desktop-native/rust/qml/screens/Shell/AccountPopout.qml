pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Templates as T
import com.yap.native

// The account popover (UserAccountPopover.tsx): your profile card over
// the account row, 8 px above it. The status line under your name edits the
// custom status (Enter or leaving the field saves, Esc cancels). The rows:
// your status (a submenu of Online, Idle, Do not disturb and Invisible),
// Copy user ID (Sign out is in Settings > Account, as on the web). Status
// changes are stored and broadcast.
T.Popup {
    id: popup

    property Item anchorItem: null
    property bool statusView: false

    readonly property string status: Session.meStatus.length > 0 ? Session.meStatus : "online"
    readonly property var options: [
        { value: "online", label: qsTr("Online") },
        { value: "idle", label: qsTr("Idle") },
        { value: "dnd", label: qsTr("Do not disturb") },
        { value: "offline", label: qsTr("Invisible") }
    ]

    function label(value) {
        return value === "offline" ? qsTr("Invisible") : Theme.presenceWord(value)
    }

    function openAbove(anchor) {
        anchorItem = anchor
        statusView = false
        Profile.show(Session.meId)
        open()
        Qt.callLater(place)
    }

    function place() {
        const overlay = T.Overlay.overlay
        if (!anchorItem || !overlay)
            return
        const at = anchorItem.mapToItem(overlay, 0, 0)
        x = Math.max(12, Math.min(at.x, overlay.width - width - 12))
        y = Math.max(12, at.y - height - 8)
    }

    parent: T.Overlay.overlay
    width: card.implicitWidth
    height: card.implicitHeight
    padding: 0
    focus: true
    closePolicy: T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutside
    onHeightChanged: if (visible) place()
    onClosed: Profile.close(Session.meId)

    TextEdit {
        id: clipboard
        visible: false
        function copy(text) {
            clipboard.text = text
            clipboard.selectAll()
            clipboard.copy()
        }
    }

    contentItem: ProfileCard {
        floating: true
        id: card
        name: Session.meName
        username: Session.meUsername
        avatar: Session.meAvatar
        initials: Session.meInitials
        pronouns: Profile.me ? Profile.pronouns : ""
        banner: Profile.me ? Profile.banner : ""
        bannerColor: Profile.me ? Profile.bannerColor : ""
        // UserAccountPopover.tsx: your card wears your collectibles,
        // name style and profile theme.
        deco: Session.meAvatar.length > 0 ? Session.meDeco : ""
        effect: Session.meEffect
        frame: Session.meFrame
        nameStyle: Session.meNameStyle
        surfaceColor: Settings.backgroundColor
        accentColor: Settings.accentColor
        animate: popup.visible
        statusLine: customStatusLine
        Accessible.role: Accessible.Dialog
        Accessible.name: qsTr("User account options")

        // The menu rows reach the card's inset edges (-mx-2 -mb-2). Both
        // views share one grid cell on the web, so the card keeps the
        // taller one's height and does not jump as they swap.
        Item {
            x: -8
            width: parent.width + 16
            height: Math.max(statusRows.implicitHeight, mainRows.implicitHeight) - 8

            Column {
                id: mainRows
                width: parent.width
                // Hidden by opacity: a hidden positioner reports no height.
                opacity: popup.statusView ? 0 : 1
                enabled: !popup.statusView
                Accessible.ignored: popup.statusView
                AccountRowKey {
                    glyph: "edit"
                    text: qsTr("Edit profile")
                    onTriggered: {
                        popup.close()
                        SettingsNav.profileEditorRequested()
                    }
                }
                AccountRowKey {
                    glyphItem: Component {
                        Led {
                            size: 10
                            lit: popup.status !== "offline"
                            color: Theme.presenceLed(popup.status)
                        }
                    }
                    text: popup.label(popup.status)
                    trailingGlyph: "chevronRight"
                    onTriggered: popup.statusView = true
                }
                Rectangle {
                    x: 4
                    width: parent.width - 8
                    height: 9
                    color: "transparent"
                    Rectangle { anchors.centerIn: parent; width: parent.width; height: 1; color: Theme.line }
                }
                AccountRowKey {
                    glyph: "copyId"
                    text: qsTr("Copy user ID")
                    onTriggered: {
                        clipboard.copy(Session.meId)
                        popup.close()
                    }
                }
            }

            Column {
                id: statusRows
                width: parent.width
                opacity: popup.statusView ? 1 : 0
                enabled: popup.statusView
                Accessible.ignored: !popup.statusView
                AccountRowKey {
                    glyph: "chevronLeft"
                    text: qsTr("Status")
                    quiet: true
                    Accessible.name: qsTr("Back to account options")
                    onTriggered: popup.statusView = false
                }
                Repeater {
                    model: popup.options
                    delegate: AccountRowKey {
                        id: option
                        required property var modelData
                        readonly property bool chosen: popup.status === modelData.value
                        glyphItem: Component {
                            Led {
                                size: 10
                                lit: option.modelData.value !== "offline"
                                color: Theme.presenceLed(option.modelData.value)
                            }
                        }
                        text: modelData.label
                        trailingGlyph: chosen ? "check" : ""
                        selected: chosen
                        Accessible.role: Accessible.RadioButton
                        Accessible.checked: chosen
                        onTriggered: {
                            Session.setStatus(option.modelData.value, Session.meCustomStatus)
                            popup.statusView = false
                        }
                    }
                }
            }
        }
    }

    // The custom status: a 28 px quiet key, or the field while editing.
    Component {
        id: customStatusLine
        Item {
            id: line
            property bool editing: false
            width: parent ? parent.width : 300
            height: editing ? 32 : 28

            function save() {
                if (!editing)
                    return
                editing = false
                const value = field.text.trim()
                if (value !== Session.meCustomStatus)
                    Session.setStatus(popup.status, value)
            }

            Rectangle {
                visible: !line.editing
                x: -6
                width: Math.min(parent.width + 12, row.implicitWidth + 12)
                height: 28
                radius: Theme.radius
                color: keyHover.hovered ? Theme.tint1 : "transparent"
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Edit custom status")
                Row {
                    id: row
                    x: 6
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 6
                    Icon {
                        anchors.verticalCenter: parent.verticalCenter
                        name: Session.meCustomStatus.length > 0 ? "emoji" : "add"
                        size: Theme.iconSm
                        color: Theme.ink3
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        width: Math.min(implicitWidth, line.width - 24)
                        textFormat: Text.PlainText
                        text: Session.meCustomStatus.length > 0 ? Session.meCustomStatus : qsTr("Set a custom status")
                        elide: Text.ElideRight
                        color: Session.meCustomStatus.length > 0 ? Theme.ink1 : Theme.ink3
                        font: Theme.type.description
                    }
                }
                HoverHandler { id: keyHover; cursorShape: Qt.PointingHandCursor }
                TapHandler {
                    onTapped: {
                        field.text = Session.meCustomStatus
                        line.editing = true
                        field.forceActiveFocus()
                    }
                }
            }

            TextField {
                id: field
                visible: line.editing
                width: parent.width
                height: 32
                maximumLength: 128
                placeholderText: qsTr("Set a custom status")
                placeholderTextColor: Theme.ink3
                color: Theme.ink1
                font: Theme.type.description
                leftPadding: 10
                selectByMouse: true
                Accessible.name: qsTr("Custom status")
                background: Rectangle {
                    radius: Theme.radius
                    color: Theme.ply2
                    border.width: 1
                    border.color: field.activeFocus ? Theme.focus : Theme.lineField
                }
                Keys.onReturnPressed: line.save()
                Keys.onEnterPressed: line.save()
                Keys.onEscapePressed: event => {
                    // Cancel the edit; the card stays open.
                    event.accepted = true
                    line.editing = false
                }
                onActiveFocusChanged: if (!activeFocus) line.save()
            }
        }
    }

    background: null

    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
            NumberAnimation { property: "scale"; from: Theme.scaleIn; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
        }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: Theme.dur3Exit }
    }

    // A 36 px menu row (ui/menu.ts MENU_ITEM): a 16 px glyph (or an LED)
    // column, Geist 14/500, an optional trailing glyph; tint-2 on hover.
    component AccountRowKey: Rectangle {
        id: key
        property string glyph: ""
        property Component glyphItem: null
        property string text
        property string trailingGlyph: ""
        property bool danger: false
        property bool quiet: false
        property bool selected: false
        signal triggered()

        width: parent ? parent.width : 200
        height: 36
        radius: Theme.radius
        color: keyHover.hovered ? (danger ? Theme.dangerTint : Theme.tint2) : selected ? Theme.tint1 : "transparent"
        Accessible.role: Accessible.MenuItem
        Accessible.name: text

        Item {
            id: lead
            x: 12
            width: 16
            height: parent.height
            Icon {
                anchors.centerIn: parent
                visible: key.glyph.length > 0
                name: key.glyph
                size: Theme.iconSm
                color: key.danger ? Theme.dangerInk : Theme.ink2
            }
            Loader {
                anchors.centerIn: parent
                active: key.glyphItem !== null
                sourceComponent: key.glyphItem
            }
        }
        Text {
            anchors.left: lead.right
            anchors.leftMargin: 12
            anchors.right: trail.left
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: key.text
            elide: Text.ElideRight
            color: key.danger ? Theme.dangerInk : key.quiet ? Theme.ink2 : Theme.ink1
            font: Theme.type.row
        }
        Icon {
            id: trail
            anchors.right: parent.right
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            visible: key.trailingGlyph.length > 0
            name: key.trailingGlyph
            size: key.trailingGlyph === "check" ? Theme.iconSm : Theme.iconXs
            color: key.trailingGlyph === "check" ? Theme.ink1 : Theme.ink3
        }
        HoverHandler { id: keyHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: key.triggered() }
    }
}
