import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The account panel (Drawer Plus): it grows out of the drawer's account
// plate, upward from the plate's own edge (0.97 to 1 and in, 160 ms
// ease-out; out in 100 ms; under reduced motion it only fades), over a
// scrim. Your card on top, your presence as a segmented rail, then the
// account rows (Yap.Ui's AccountMenuRow, the desktop's account menu rows):
// your custom status (edited in place), your profile, the theme (a tap
// swaps its light and dark), and Settings, the one notched primary key.
// The plate's own rows stay at its foot, where the plate is, so nothing
// jumps. Back, its predictive gesture, a tap on the scrim or on the rows
// at its foot close it.
Item {
    id: root

    signal settingsRequested()
    signal profileRequested(string userId)
    signal callRequested()

    property bool shown: false
    // The system back gesture's progress while it is open (Main.qml).
    property real backProgress: 0
    // The plate it grows out of, and where that stands in this item.
    property Item plate: null
    property rect from: Qt.rect(0, 0, 0, 0)

    readonly property string status: Session.meStatus.length > 0 ? Session.meStatus : "online"
    // Read here, where the panel fills the window: the card asking for its
    // own would feed its position back in.
    readonly property real topInset: SafeArea.margins.top
    readonly property var themes: ({
        "dark": qsTr("Midnight"),
        "light": qsTr("Light"),
        "miku-dark": qsTr("Miku Dark"),
        "miku-light": qsTr("Miku Light"),
        "spiderman-dark": qsTr("Spider-Man Dark"),
        "spiderman-light": qsTr("Spider-Man Light")
    })
    // The same colourway on the other ground.
    readonly property string otherTheme: Theme.themeId.endsWith("-dark") ? Theme.themeId.replace(/-dark$/, "-light")
        : Theme.themeId.endsWith("-light") ? Theme.themeId.replace(/-light$/, "-dark")
        : Theme.themeId === "light" ? "dark" : "light"

    function openFrom(item) {
        if (!item)
            return
        plate = item
        place()
        statusField.editing = false
        backProgress = 0
        shown = true
    }
    function close() {
        if (!shown)
            return
        if (statusField.editing)
            statusField.save()
        Qt.inputMethod.hide()
        shown = false
        backProgress = 0
    }
    function place() {
        if (!plate)
            return
        const at = plate.mapToItem(root, 0, 0)
        from = Qt.rect(at.x, at.y, plate.width, plate.height)
    }
    onWidthChanged: if (shown) place()
    onHeightChanged: if (shown) place()

    visible: shown || grow > 0
    // 0 closed, 1 open: the card's scale and opacity, the scrim's.
    property real grow: shown ? 1 : 0
    Behavior on grow {
        NumberAnimation {
            duration: root.shown ? Theme.durPopover : Theme.durPopoverExit
            easing.type: Easing.BezierSpline
            easing.bezierCurve: Theme.easeOut
        }
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.scrim
        opacity: root.grow * (1 - 0.5 * root.backProgress)
        MouseArea {
            anchors.fill: parent
            enabled: root.shown
            onClicked: root.close()
        }
    }

    FloatPlate {
        id: card
        rule: true
        x: root.from.x
        width: root.from.width
        height: content.height
        // The keyboard (a custom status being typed) lifts it clear.
        readonly property real foot: root.from.y + root.from.height
        readonly property real lift: Math.max(0, Native.imeHeight + 8 - (root.height - foot))
        // Never above the status bar (a short screen at a large text size).
        y: Math.max(root.topInset + 8, foot - height - lift)
        transformOrigin: Item.Bottom
        scale: (Theme.scaleIn + (1 - Theme.scaleIn) * root.grow) * (1 - (Theme.reducedMotion ? 0 : 0.05) * root.backProgress)
        opacity: root.grow * (1 - 0.3 * root.backProgress)
        enabled: root.shown
        Accessible.role: Accessible.Dialog
        Accessible.name: qsTr("Account options")

        // Taps inside the card stay in it (not the scrim's).
        MouseArea { anchors.fill: parent }

        Column {
            id: content
            width: parent.width

            // Your card: the avatar, your name and handle, the close key.
            Item {
                width: parent.width
                height: Math.max(48, names.implicitHeight) + 24
                PresenceAvatar {
                    id: face
                    x: 14
                    y: 14
                    size: 48
                    initials: Session.meInitials.length > 0 ? Session.meInitials : "?"
                    source: Session.meAvatar
                    deco: Session.meDeco
                    presence: root.status
                    ground: Theme.ply3
                }
                Column {
                    id: names
                    anchors.left: face.right
                    anchors.leftMargin: 12
                    anchors.right: closeKey.left
                    anchors.rightMargin: 4
                    anchors.verticalCenter: face.verticalCenter
                    spacing: 2
                    DisplayName {
                        width: parent.width
                        text: Session.meName
                        nameStyle: Session.meNameStyle
                        color: Theme.ink1
                        font: Theme.type.sectionTitle
                        ground: Theme.ply3
                    }
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: Session.meUsername
                        elide: Text.ElideRight
                        color: Theme.ink3
                        font: Theme.type.description
                    }
                }
                IconKey {
                    id: closeKey
                    anchors.right: parent.right
                    anchors.rightMargin: 4
                    anchors.verticalCenter: face.verticalCenter
                    iconName: "close"
                    tip: qsTr("Close")
                    Accessible.name: qsTr("Close account options")
                    onClicked: root.close()
                }
            }

            Item {
                width: parent.width
                height: rail.height + 12
                PresenceRail {
                    id: rail
                    x: 12
                    y: 4
                    width: parent.width - 24
                    status: root.status
                    onChose: value => {
                        if (value === root.status)
                            return
                        Native.haptic(Native.Tick)
                        Session.setStatus(value, Session.meCustomStatus)
                    }
                }
            }

            // Your custom status: the row, or the field while it is typed.
            Item {
                id: statusField
                property bool editing: false
                width: parent.width
                height: editing ? field.height + 8 : customRow.height
                function save() {
                    if (!editing)
                        return
                    editing = false
                    const value = field.text.trim()
                    if (value !== Session.meCustomStatus)
                        Session.setStatus(root.status, value)
                }
                AccountMenuRow {
                    id: customRow
                    x: 4
                    width: parent.width - 8
                    visible: !statusField.editing
                    glyph: "emoji"
                    text: Session.meCustomStatus.length > 0 ? Session.meCustomStatus : qsTr("Set a custom status")
                    quiet: Session.meCustomStatus.length === 0
                    onTriggered: {
                        field.text = Session.meCustomStatus
                        statusField.editing = true
                        field.forceActiveFocus()
                        Qt.inputMethod.show()
                    }
                }
                MobileField {
                    id: field
                    x: 12
                    y: 4
                    width: parent.width - 24
                    visible: statusField.editing
                    maximumLength: 128
                    placeholderText: qsTr("Set a custom status")
                    EnterKey.type: Qt.EnterKeyDone
                    Accessible.name: qsTr("Custom status")
                    onAccepted: {
                        statusField.save()
                        Qt.inputMethod.hide()
                    }
                    onActiveFocusChanged: if (!activeFocus) statusField.save()
                }
            }
            AccountMenuRow {
                x: 4
                width: parent.width - 8
                glyph: "profile"
                text: qsTr("View profile")
                onTriggered: root.profileRequested(Session.meId)
            }
            AccountMenuRow {
                x: 4
                width: parent.width - 8
                glyph: "appearance"
                text: qsTr("Theme")
                hint: root.themes[Theme.themeId] ?? ""
                Accessible.name: qsTr("Theme, %1. Switch to %2").arg(root.themes[Theme.themeId] ?? "")
                    .arg(root.themes[root.otherTheme] ?? "")
                onTriggered: {
                    Native.haptic(Native.Tick)
                    Settings.chooseTheme(root.otherTheme)
                }
            }

            Item {
                width: parent.width
                height: settingsKey.height + 20
                KitButton {
                    id: settingsKey
                    x: 12
                    y: 8
                    width: parent.width - 24
                    variant: "primary"
                    size: "lg"
                    text: qsTr("Settings")
                    ground: Theme.ply3
                    onClicked: root.settingsRequested()
                }
            }

            // The plate's rows, where the plate is.
            Rectangle {
                width: parent.width
                height: 1
                color: Theme.line
            }
            AccountPlate {
                width: parent.width
                inPanel: true
                clockShown: root.shown
                onPanelRequested: root.close()
                onCallRequested: root.callRequested()
            }
        }
    }
}
