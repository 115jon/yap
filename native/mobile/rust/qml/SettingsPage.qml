pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Settings, one scrolling page (the desktop's Settings dialog, phone
// layout): who you are and Edit profile (ProfileEditPage), then numbered
// sections under 2 px ink rules (the desktop's rule cards). Status and its
// custom line (Session.setStatus);
// Appearance (the core's themes, kept on this device in appearance.json
// and synced to the account when asked); Text size (a factor over the
// system's, this phone's own); Notifications (the push shim's permission
// state); Devices (the account's sessions, others signed out after a
// confirm); About; Sign out, confirmed in place; Delete account, behind
// the username typed to confirm it. Opened from the account row at the
// drawer's foot.
Rectangle {
    id: root

    readonly property bool sheet: false
    // The predictive back gesture's progress (0..1): the page follows the
    // finger before the pop commits, as an Android activity does.
    property real backProgress: 0

    signal closeRequested()
    // Edit profile, from the account at the top.
    signal profileEditRequested()

    // The window's insets where this page meets its edges, read once here:
    // a child that asks for its own would feed its position back in.
    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: SafeArea.margins.bottom

    readonly property string status: Session.meStatus.length > 0 ? Session.meStatus : "online"
    readonly property var statuses: [
        { value: "online", label: qsTr("Online"), hint: "" },
        { value: "idle", label: qsTr("Idle"), hint: "" },
        { value: "dnd", label: qsTr("Do not disturb"), hint: qsTr("Mutes notifications") },
        { value: "offline", label: qsTr("Invisible"), hint: qsTr("You show as offline") }
    ]
    // The core's themes (yap_core::settings::APP_THEMES), in pairs per
    // palette after Match system; labels as the desktop's SettingsNav.
    readonly property var themes: [
        { id: "dark", label: qsTr("Midnight") },
        { id: "light", label: qsTr("Light") },
        { id: "miku-dark", label: qsTr("Miku Dark") },
        { id: "miku-light", label: qsTr("Miku Light") },
        { id: "spiderman-dark", label: qsTr("Spider-Man Dark") },
        { id: "spiderman-light", label: qsTr("Spider-Man Light") }
    ]
    // Hand-matched to each theme's tokens for the thumbnails (the desktop
    // AppearancePanel's SWATCHES).
    readonly property var swatches: ({
        "dark": { bg: "#0b0b0d", rail: "#000000", sidebar: "#141417", accent: "#ff8ad8", text: "#f2f2f3", dim: "#3a3a40" },
        "light": { bg: "#fafafa", rail: "#f4f4f5", sidebar: "#f0f0f2", accent: "#c0127a", text: "#0b0b0d", dim: "#c9c9cf" },
        "miku-dark": { bg: "#0c0b14", rail: "#07060d", sidebar: "#0f0e1a", accent: "#39c5bb", text: "#ffffff", dim: "#34324a" },
        "miku-light": { bg: "#f3faf9", rail: "#d3ede8", sidebar: "#e1f3f0", accent: "#00a399", text: "#1b2240", dim: "#b5d6d1" },
        "spiderman-dark": { bg: "#020208", rail: "#000000", sidebar: "#03030b", accent: "#e50914", text: "#ffffff", dim: "#1d2a52" },
        "spiderman-light": { bg: "#f8f9fc", rail: "#e8ebf3", sidebar: "#f1f3f9", accent: "#e50914", text: "#0f172a", dim: "#c7cddd" }
    })
    // Text size factors over the system's scale.
    readonly property var textSizes: [
        { factor: 0.85, label: qsTr("Small") },
        { factor: 1, label: qsTr("Default") },
        { factor: 1.15, label: qsTr("Large") },
        { factor: 1.3, label: qsTr("Larger") }
    ]

    readonly property var sessions: {
        try {
            return JSON.parse(Settings.sessions)
        } catch (e) {
            return []
        }
    }
    readonly property var revoke: {
        try {
            return JSON.parse(Extras.account)
        } catch (e) {
            return {}
        }
    }
    // The other device whose Sign out is asking first, or empty.
    property string armedSession: ""
    property bool signOutArmed: false
    property bool deleteArmed: false

    color: Theme.ply0
    transform: [
        Scale {
            origin.x: root.width / 2
            origin.y: root.height / 2
            xScale: 1 - root.backProgress * 0.08
            yScale: 1 - root.backProgress * 0.08
        },
        Translate { x: root.backProgress * root.width * 0.1 }
    ]

    Component.onCompleted: Settings.loadSessions()

    PageHeader {
        id: header
        z: 1
        title: qsTr("Settings")
        topInset: root.topInset
        leftInset: root.leftInset
        rightInset: root.rightInset
        onBackRequested: root.closeRequested()
    }

    Flickable {
        id: scroll
        anchors.top: header.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        contentHeight: body.height
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickDeceleration: Native.flingDeceleration
        maximumFlickVelocity: Native.maxFlingVelocity
        // The sign-out question opens under the page's last key: the page
        // follows it down once, or its two keys sat under the gesture bar.
        property bool revealEnd: false
        onContentHeightChanged: if (revealEnd) {
            revealEnd = false
            contentY = Math.max(0, contentHeight - height)
        }

        Column {
            id: body
            x: Math.max(16 + root.leftInset, (scroll.width - width) / 2)
            width: Math.min(scroll.width - 32 - root.leftInset - root.rightInset, 560)
            topPadding: 20
            bottomPadding: 32 + root.bottomInset
            spacing: 32

            // Who is signed in, and Edit profile.
            Column {
                width: parent.width
                spacing: 16
                Row {
                    width: parent.width
                    spacing: 16
                    PresenceAvatar {
                        id: face
                        anchors.verticalCenter: parent.verticalCenter
                        size: 64
                        initials: Session.meInitials.length > 0 ? Session.meInitials : "?"
                        source: Session.meAvatar
                        deco: Session.meDeco
                        presence: root.status
                        ground: root.color
                    }
                    Column {
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - face.width - parent.spacing
                        spacing: 2
                        DisplayName {
                            width: parent.width
                            text: Session.meName
                            nameStyle: Session.meNameStyle
                            color: Theme.ink1
                            font: Theme.type.pageTitle
                            ground: root.color
                        }
                        Text {
                            width: parent.width
                            textFormat: Text.PlainText
                            text: "@" + Session.meUsername
                            elide: Text.ElideRight
                            color: Theme.ink2
                            font: Theme.type.description
                        }
                    }
                }
                KitButton {
                    width: parent.width
                    variant: "secondary"
                    iconName: "edit"
                    text: qsTr("Edit profile")
                    onClicked: root.profileEditRequested()
                }
            }

            Section {
                number: 1
                title: qsTr("Status")
                Repeater {
                    model: root.statuses
                    delegate: ChoiceRow {
                        required property var modelData
                        text: modelData.label
                        hint: modelData.hint
                        chosen: root.status === modelData.value
                        led: modelData.value
                        onChose: Session.setStatus(modelData.value, Session.meCustomStatus)
                    }
                }
                Item { width: 1; height: 8 }
                Item {
                    width: parent.width
                    height: statusField.height
                    MobileField {
                        id: statusField
                        width: parent.width
                        rightPadding: clearKey.visible ? 48 : 14
                        maximumLength: 128
                        placeholderText: qsTr("Set a custom status")
                        EnterKey.type: Qt.EnterKeyDone
                        Accessible.name: qsTr("Custom status")
                        Component.onCompleted: text = Session.meCustomStatus
                        function save() {
                            const value = text.trim()
                            if (value !== Session.meCustomStatus)
                                Session.setStatus(root.status, value)
                        }
                        onAccepted: {
                            save()
                            focus = false
                            Qt.inputMethod.hide()
                        }
                        onActiveFocusChanged: if (!activeFocus) save()
                    }
                    IconKey {
                        id: clearKey
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        visible: statusField.text.length > 0
                        iconName: "close"
                        tip: qsTr("Clear custom status")
                        onClicked: {
                            statusField.text = ""
                            Session.setStatus(root.status, "")
                        }
                    }
                    // The server's value, unless it is being typed over.
                    Connections {
                        target: Session
                        function onMeCustomStatusChanged() {
                            if (!statusField.activeFocus)
                                statusField.text = Session.meCustomStatus
                        }
                    }
                }
            }

            Section {
                number: 2
                title: qsTr("Appearance")
                ThemeTile {
                    width: parent.width
                    themeId: "system"
                    label: qsTr("Match system")
                }
                Item { width: 1; height: 12 }
                Grid {
                    width: parent.width
                    columns: 2
                    columnSpacing: 12
                    rowSpacing: 12
                    Repeater {
                        model: root.themes
                        delegate: ThemeTile {
                            required property var modelData
                            width: (parent.width - 12) / 2
                            themeId: modelData.id
                            label: modelData.label
                        }
                    }
                }
                Item { width: 1; height: 8 }
                SwitchItem {
                    text: qsTr("Use on all my devices")
                    description: qsTr("Your theme follows your account.")
                    checked: Settings.themeSync
                    onToggled: Settings.changeThemeSync(!Settings.themeSync)
                }
            }

            Section {
                number: 3
                title: qsTr("Text size")
                Segmented {
                    width: parent.width
                    accessibleName: qsTr("Text size")
                    options: root.textSizes.map(size => size.label)
                    current: {
                        const at = root.textSizes.findIndex(size => Math.abs(size.factor - Prefs.textFactor) < 0.01)
                        return at < 0 ? 1 : at
                    }
                    onPicked: index => Prefs.textFactor = root.textSizes[index].factor
                }
                Item { width: 1; height: 12 }
                // A message at the chosen size (the type roles already carry
                // Theme.textScale).
                Rectangle {
                    width: parent.width
                    height: sample.height + 24
                    radius: Theme.radius
                    color: Theme.ply1
                    border.width: 1
                    border.color: Theme.line
                    Row {
                        id: sample
                        x: 12
                        y: 12
                        width: parent.width - 24
                        spacing: 12
                        Avatar {
                            id: sampleFace
                            size: 32
                            initials: Session.meInitials.length > 0 ? Session.meInitials : "?"
                            source: Session.meAvatar
                        }
                        Column {
                            width: parent.width - sampleFace.width - parent.spacing
                            spacing: 2
                            Text {
                                width: parent.width
                                textFormat: Text.PlainText
                                text: Session.meName
                                elide: Text.ElideRight
                                color: Theme.ink1
                                font: Theme.type.author
                            }
                            Text {
                                width: parent.width
                                textFormat: Text.PlainText
                                text: qsTr("Messages, names and menus read at this size.")
                                wrapMode: Text.Wrap
                                color: Theme.ink1
                                font: Theme.type.message
                            }
                        }
                    }
                }
                Item { width: 1; height: 8 }
                Text {
                    width: parent.width
                    wrapMode: Text.Wrap
                    text: qsTr("On top of your phone's text size (%1%).").arg(Math.round(Native.fontScale * 100))
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }

            Section {
                number: 4
                title: qsTr("Notifications")
                SwitchItem {
                    text: qsTr("Push notifications")
                    description: Native.notificationsEnabled
                        ? qsTr("New messages reach this phone. Turn them off in Android's settings.")
                        : qsTr("Turn them on to hear about new messages.")
                    checked: Native.notificationsEnabled
                    // Once on, only Android's settings turn them off.
                    locked: Native.notificationsEnabled
                    // The permission dialog while Android still shows it,
                    // else the app's notification settings (the shim).
                    onToggled: Native.requestNotifications()
                }
            }

            Section {
                number: 5
                title: qsTr("Devices")
                Loader {
                    width: parent.width
                    active: Settings.sessionsState === "loading" && root.sessions.length === 0
                    visible: active
                    sourceComponent: Item {
                        height: 64
                        Spinner {
                            anchors.centerIn: parent
                            running: true
                        }
                    }
                }
                Column {
                    width: parent.width
                    visible: Settings.sessionsState === "error"
                    spacing: 8
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: Settings.sessionsError
                        wrapMode: Text.Wrap
                        color: Theme.dangerInk
                        font: Theme.type.description
                    }
                    KitButton {
                        iconName: "refresh"
                        text: qsTr("Try again")
                        loading: Settings.sessionsState === "loading"
                        onClicked: Settings.loadSessions()
                    }
                }
                Repeater {
                    // This phone first.
                    model: Settings.sessionsState === "error" ? []
                        : root.sessions.filter(s => s.current).concat(root.sessions.filter(s => !s.current))
                    delegate: SessionItem {}
                }
            }

            Section {
                number: 6
                title: qsTr("About")
                InfoRow {
                    text: qsTr("Version")
                    value: Qt.application.version.length > 0 ? Qt.application.version : Settings.version
                }
                LinkRow {
                    text: qsTr("Licence")
                    url: "https://github.com/115jon/yap/blob/main/LICENSE"
                }
                LinkRow {
                    text: qsTr("Privacy")
                    url: "https://github.com/115jon/yap/blob/main/PRIVACY.md"
                }
            }

            Column {
                width: parent.width
                spacing: 12
                KitButton {
                    width: parent.width
                    visible: !root.signOutArmed
                    size: "lg"
                    variant: "destructive"
                    iconName: "logout"
                    text: qsTr("Sign out")
                    onClicked: {
                        scroll.revealEnd = true
                        root.signOutArmed = true
                    }
                }
                Loader {
                    width: parent.width
                    active: root.signOutArmed
                    visible: active
                    sourceComponent: InlineConfirm {
                        consequence: qsTr("Sign out of Yap on this phone? Your messages stay on your account.")
                        confirmLabel: qsTr("Sign out")
                        onCancelled: {
                            scroll.revealEnd = false
                            root.signOutArmed = false
                        }
                        onConfirmed: Session.signOut()
                    }
                }
            }

            // Delete account (the desktop's Account → Delete account): the
            // warning with the date 30 days out, the username typed to
            // confirm it, then the request; once scheduled this phone signs
            // out as Sign out does. A refusal stays under the keys.
            Section {
                number: 7
                title: qsTr("Delete account")
                Text {
                    width: parent.width
                    bottomPadding: 12
                    textFormat: Text.PlainText
                    text: qsTr("Deletes your Yap account after 30 days. Sign in before then to restore it.")
                    wrapMode: Text.Wrap
                    color: Theme.ink2
                    font: Theme.type.description
                }
                KitButton {
                    width: parent.width
                    visible: !root.deleteArmed
                    size: "lg"
                    variant: "destructive"
                    iconName: "trash"
                    text: qsTr("Delete account")
                    onClicked: {
                        scroll.revealEnd = true
                        root.deleteArmed = true
                    }
                }
                Loader {
                    width: parent.width
                    active: root.deleteArmed
                    visible: active
                    sourceComponent: DeleteConfirm {
                        onCancelled: {
                            scroll.revealEnd = false
                            root.deleteArmed = false
                            Session.dismissDeleteAccount()
                        }
                    }
                }
            }
        }
    }

    // The delete confirm: InlineConfirm's look with the username field
    // between the warning and the keys; Delete account works only once the
    // field names this account (any case, an optional @).
    component DeleteConfirm: Item {
        id: confirm

        readonly property var deletion: {
            try {
                return JSON.parse(Session.deletion.length > 0 ? Session.deletion : "{}")
            } catch (e) {
                return {}
            }
        }
        readonly property bool busy: deletion.deleting === true
        readonly property string error: deletion.deleteError ?? ""
        // Fixed as it opens, as the web's warning does.
        readonly property string deleteOn: Session.deletionDateFromNow()
        readonly property bool matches: Session.confirmsUsername(nameField.text)

        signal cancelled()

        function submit() {
            if (confirm.busy || !confirm.matches)
                return
            Native.haptic(Native.Confirm)
            Session.deleteAccount(nameField.text)
        }

        width: parent ? parent.width : 0
        implicitHeight: box.height

        Rectangle {
            id: box
            width: parent.width
            height: content.implicitHeight + 32
            radius: Theme.radius
            color: Theme.tint1
            Rectangle {
                width: 2
                height: parent.height
                color: Theme.dangerInk
            }
            Column {
                id: content
                x: 16
                y: 16
                width: parent.width - 32
                spacing: 12
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: qsTr("Delete your account on %1?").arg(confirm.deleteOn)
                    wrapMode: Text.Wrap
                    color: Theme.ink1
                    font: Theme.type.setting
                    Accessible.role: Accessible.Heading
                    Accessible.name: text
                }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: qsTr("You leave every server, are shown to others as “Deleted user” and are signed out everywhere now. On that date your profile, board, friends, settings and uploaded files are deleted. Messages you sent stay, from “Deleted user”. Sign in before then to restore your account. Your Kova sign-in is not deleted.")
                    wrapMode: Text.Wrap
                    color: Theme.ink1
                    font: Theme.type.description
                    Accessible.role: Accessible.StaticText
                    Accessible.name: text
                }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: qsTr("Type @%1 to confirm.").arg(Session.meUsername)
                    wrapMode: Text.Wrap
                    color: Theme.ink2
                    font: Theme.type.description
                }
                MobileField {
                    id: nameField
                    width: parent.width
                    maximumLength: 64
                    enabled: !confirm.busy
                    placeholderText: "@" + Session.meUsername
                    inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText | Qt.ImhSensitiveData
                    Accessible.name: qsTr("Type %1 to confirm").arg(Session.meUsername)
                    onAccepted: confirm.submit()
                }
                Row {
                    width: parent.width
                    spacing: 8
                    KitButton {
                        width: (parent.width - parent.spacing) / 2
                        size: "lg"
                        variant: "secondary"
                        text: qsTr("Cancel")
                        enabled: !confirm.busy
                        onClicked: confirm.cancelled()
                    }
                    KitButton {
                        width: (parent.width - parent.spacing) / 2
                        size: "lg"
                        variant: "danger"
                        text: qsTr("Delete account")
                        enabled: confirm.matches
                        loading: confirm.busy
                        onClicked: confirm.submit()
                    }
                }
                Text {
                    width: parent.width
                    visible: confirm.error.length > 0
                    textFormat: Text.PlainText
                    text: confirm.error
                    wrapMode: Text.Wrap
                    color: Theme.dangerInk
                    font: Theme.type.caption
                    Accessible.role: Accessible.AlertMessage
                    Accessible.name: text
                }
            }
        }
    }

    // A numbered section (the desktop's rule card): a 2 px ink-1 rule, the
    // silkscreen legend and its "01" on the rule line, then the rows.
    component Section: Column {
        id: section
        property int number: 0
        property string title
        default property alias content: rows.data
        width: parent ? parent.width : 0
        spacing: 0
        Rectangle {
            width: parent.width
            height: 2
            color: Theme.ink1
        }
        Item {
            width: parent.width
            height: 36
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: section.title
                color: Theme.ink2
                font: Theme.type.silk
                Accessible.role: Accessible.Heading
                Accessible.name: section.title
            }
            Text {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                text: String(section.number).padStart(2, "0")
                color: Theme.ink3
                font: Theme.type.numSm
                Accessible.ignored: true
            }
        }
        Column {
            id: rows
            width: parent.width
        }
    }

    // One of a set (a status): the LED, the label over an optional hint,
    // the check while chosen. 48 high, the whole row is the key.
    component ChoiceRow: Item {
        id: choice
        property string text
        property string hint
        property string led
        property bool chosen: false
        signal chose()
        width: parent ? parent.width : 0
        height: Math.max(48, labels.implicitHeight + 16)
        Accessible.role: Accessible.RadioButton
        Accessible.name: hint.length > 0 ? text + ", " + hint : text
        Accessible.checkable: true
        Accessible.checked: chosen
        Accessible.onPressAction: choice.chose()
        Rectangle {
            anchors.fill: parent
            anchors.leftMargin: -8
            anchors.rightMargin: -8
            radius: Theme.radius
            color: choiceTap.pressed ? Theme.tint2 : choice.chosen ? Theme.tint1 : "transparent"
            Behavior on color { ColorAnimation { duration: Theme.dur1 } }
        }
        Led {
            x: 4
            anchors.verticalCenter: parent.verticalCenter
            size: 10
            lit: choice.led !== "offline"
            color: Theme.presenceLed(choice.led)
        }
        Column {
            id: labels
            anchors.left: parent.left
            anchors.leftMargin: 30
            anchors.right: check.left
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: choice.text
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.setting
            }
            Text {
                width: parent.width
                visible: choice.hint.length > 0
                textFormat: Text.PlainText
                text: choice.hint
                elide: Text.ElideRight
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
        Icon {
            id: check
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            visible: choice.chosen
            name: "check"
            size: Theme.iconLg
            color: Theme.ink1
        }
        TapHandler {
            id: choiceTap
            onTapped: {
                if (choice.chosen)
                    return
                Native.haptic(Native.Tick)
                choice.chose()
            }
        }
    }

    // A setting you switch: label and description, the LED switch at the
    // end; the whole row is the key.
    component SwitchItem: Item {
        id: item
        property string text
        property string description
        property bool checked: false
        // Shown, but a tap changes nothing.
        property bool locked: false
        signal toggled()
        width: parent ? parent.width : 0
        height: Math.max(56, itemLabels.implicitHeight + 16)
        Accessible.role: Accessible.CheckBox
        Accessible.name: text
        Accessible.description: description
        Accessible.checkable: true
        Accessible.checked: checked
        Accessible.onPressAction: if (!item.locked) item.toggled()
        Rectangle {
            anchors.fill: parent
            anchors.leftMargin: -8
            anchors.rightMargin: -8
            radius: Theme.radius
            color: itemTap.pressed && !item.locked ? Theme.tint2 : "transparent"
            Behavior on color { ColorAnimation { duration: Theme.dur1 } }
        }
        Column {
            id: itemLabels
            anchors.left: parent.left
            anchors.right: toggle.left
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: item.text
                wrapMode: Text.Wrap
                color: Theme.ink1
                font: Theme.type.setting
            }
            Text {
                width: parent.width
                visible: item.description.length > 0
                textFormat: Text.PlainText
                text: item.description
                wrapMode: Text.Wrap
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
        SettingSwitch {
            id: toggle
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            checked: item.checked
            // The row takes the tap.
            enabled: false
            opacity: 1
            Accessible.ignored: true
        }
        TapHandler {
            id: itemTap
            onTapped: {
                if (item.locked)
                    return
                Native.haptic(Native.Tick)
                item.toggled()
            }
        }
    }

    // A theme's thumbnail (its own colours; Match system split dark and
    // light) over its name, ringed in ink-1 with a check while chosen.
    component ThemeTile: Item {
        id: tile
        property string themeId
        property string label
        readonly property bool on: Settings.theme === themeId
        height: thumb.height + 8 + 18
        Accessible.role: Accessible.RadioButton
        Accessible.name: label
        Accessible.checkable: true
        Accessible.checked: on
        Accessible.onPressAction: Settings.chooseTheme(tile.themeId)
        scale: tileTap.pressed ? Theme.scalePress : 1
        Behavior on scale { NumberAnimation { duration: Theme.dur1 } }
        Rectangle {
            id: thumb
            width: parent.width
            height: tile.themeId === "system" ? 72 : Math.round(width * 9 / 16)
            radius: Theme.radius
            color: "transparent"
            clip: true
            Item {
                anchors.fill: parent
                anchors.margins: 2
                clip: true
                Loader {
                    width: parent.width
                    height: parent.height
                    active: tile.themeId !== "system"
                    sourceComponent: Mock { t: root.swatches[tile.themeId] }
                }
                Item {
                    visible: tile.themeId === "system"
                    width: parent.width / 2
                    height: parent.height
                    clip: true
                    Mock { width: parent.width * 2; height: parent.height; t: root.swatches["dark"] }
                }
                Item {
                    visible: tile.themeId === "system"
                    x: parent.width / 2
                    width: parent.width / 2
                    height: parent.height
                    clip: true
                    Mock { x: -parent.width; width: parent.width * 2; height: parent.height; t: root.swatches["light"] }
                }
            }
            Rectangle {
                anchors.fill: parent
                color: "transparent"
                radius: Theme.radius
                border.width: tile.on ? 2 : 1
                border.color: tile.on ? Theme.ink1 : Theme.lineStrong
            }
            Rectangle {
                visible: tile.on
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 6
                width: 20
                height: 20
                radius: Theme.radius
                color: Theme.primaryFill
                Icon {
                    anchors.centerIn: parent
                    name: "check"
                    size: Theme.iconXs
                    color: Theme.primaryFillInk
                }
            }
        }
        Text {
            y: thumb.height + 8
            width: parent.width
            textFormat: Text.PlainText
            text: tile.label
            elide: Text.ElideRight
            color: tile.on ? Theme.ink1 : Theme.ink2
            font: tile.on ? Theme.type.labelStrong : Theme.type.description
        }
        TapHandler {
            id: tileTap
            onTapped: {
                if (tile.on)
                    return
                Native.haptic(Native.Tick)
                Settings.chooseTheme(tile.themeId)
            }
        }
    }

    // A theme drawn small: the rail, the sidebar, a header line, two
    // messages and the accent.
    component Mock: Rectangle {
        id: mock
        required property var t
        color: t.bg
        Rectangle { width: parent.width * 0.1; height: parent.height; color: mock.t.rail }
        Rectangle { x: parent.width * 0.1; width: parent.width * 0.26; height: parent.height; color: mock.t.sidebar }
        Rectangle { x: parent.width * 0.14; y: parent.height * 0.18; width: parent.width * 0.16; height: 3; color: mock.t.dim }
        Rectangle { x: parent.width * 0.14; y: parent.height * 0.32; width: parent.width * 0.12; height: 3; color: mock.t.accent }
        Rectangle { x: parent.width * 0.14; y: parent.height * 0.46; width: parent.width * 0.18; height: 3; color: mock.t.dim }
        Rectangle { x: parent.width * 0.42; y: parent.height * 0.24; width: parent.width * 0.32; height: 4; color: mock.t.text; opacity: 0.85 }
        Rectangle { x: parent.width * 0.42; y: parent.height * 0.38; width: parent.width * 0.48; height: 3; color: mock.t.dim }
        Rectangle { x: parent.width * 0.42; y: parent.height * 0.58; width: parent.width * 0.26; height: 4; color: mock.t.text; opacity: 0.85 }
        Rectangle { x: parent.width * 0.42; y: parent.height * 0.72; width: parent.width * 0.4; height: 3; color: mock.t.dim }
        Rectangle { x: parent.width * 0.06 - 2; y: parent.height * 0.14; width: 4; height: 4; color: mock.t.accent }
    }

    // A choice of two to four (DESIGN.md Segmented rail): a 2 px ink-1 rail,
    // cells split by 2 px rules, the chosen cell lit in primary-fill.
    component Segmented: Rectangle {
        id: rail
        property var options: []
        property int current: 0
        property string accessibleName
        signal picked(int index)
        height: 48
        radius: Theme.radius
        color: "transparent"
        border.width: 2
        border.color: Theme.ink1
        Accessible.role: Accessible.Grouping
        Accessible.name: accessibleName
        Row {
            x: 2
            y: 2
            width: parent.width - 4
            height: parent.height - 4
            Repeater {
                model: rail.options
                delegate: Item {
                    id: cell
                    required property int index
                    required property string modelData
                    readonly property bool on: rail.current === index
                    width: parent.width / rail.options.length
                    height: parent.height
                    Accessible.role: Accessible.RadioButton
                    Accessible.name: modelData
                    Accessible.checkable: true
                    Accessible.checked: on
                    Accessible.onPressAction: rail.picked(index)
                    Rectangle {
                        anchors.fill: parent
                        color: cell.on ? Theme.primaryFill : cellTap.pressed ? Theme.tint2 : "transparent"
                        Behavior on color { ColorAnimation { duration: Theme.dur1 } }
                    }
                    Rectangle {
                        visible: cell.index > 0
                        width: 2
                        height: parent.height
                        color: Theme.ink1
                    }
                    Text {
                        anchors.centerIn: parent
                        width: parent.width - 8
                        horizontalAlignment: Text.AlignHCenter
                        textFormat: Text.PlainText
                        text: cell.modelData
                        elide: Text.ElideRight
                        color: cell.on ? Theme.primaryFillInk : Theme.ink1
                        font: Theme.type.controlSm
                    }
                    TapHandler {
                        id: cellTap
                        onTapped: {
                            if (cell.on)
                                return
                            Native.haptic(Native.Tick)
                            rail.picked(cell.index)
                        }
                    }
                }
            }
        }
    }

    // A device signed in to the account: its glyph, name, where and when;
    // another device's Sign out asks first.
    component SessionItem: Column {
        id: device
        required property var modelData
        readonly property bool armed: root.armedSession === modelData.id
        width: parent ? parent.width : 0
        bottomPadding: armed ? 12 : 0
        Item {
            width: parent.width
            height: Math.max(64, deviceLabels.implicitHeight + 16)
            Accessible.role: Accessible.ListItem
            Accessible.name: device.modelData.title + ", " + device.modelData.detail
            Rectangle {
                id: glyph
                anchors.verticalCenter: parent.verticalCenter
                width: 40
                height: 40
                radius: Theme.radius
                color: Theme.tint2
                Icon {
                    anchors.centerIn: parent
                    name: device.modelData.mobile ? "mobile" : "monitor"
                    size: Theme.iconMd
                    color: Theme.ink1
                }
            }
            Column {
                id: deviceLabels
                anchors.left: glyph.right
                anchors.leftMargin: 12
                anchors.right: signOutKey.visible ? signOutKey.left : parent.right
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Row {
                    width: parent.width
                    spacing: 8
                    Text {
                        width: Math.min(implicitWidth, parent.width - (here.visible ? here.width + 8 : 0))
                        textFormat: Text.PlainText
                        text: device.modelData.title
                        elide: Text.ElideRight
                        color: Theme.ink1
                        font: Theme.type.setting
                    }
                    Text {
                        id: here
                        visible: device.modelData.current
                        anchors.verticalCenter: parent.verticalCenter
                        text: qsTr("This device")
                        color: Theme.successInk
                        font: Theme.type.silk
                    }
                }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: device.modelData.detail
                    elide: Text.ElideRight
                    color: Theme.ink2
                    font: Theme.type.description
                }
            }
            KitButton {
                id: signOutKey
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                visible: !device.modelData.current && !device.armed
                variant: "secondary"
                text: qsTr("Sign out")
                Accessible.name: qsTr("Sign out %1").arg(device.modelData.title)
                onClicked: root.armedSession = device.modelData.id
            }
        }
        Loader {
            width: parent.width
            active: device.armed
            visible: active
            sourceComponent: InlineConfirm {
                consequence: qsTr("%1 will be signed out and need to sign in again.").arg(device.modelData.title)
                confirmLabel: qsTr("Sign out device")
                busy: root.revoke.revoking === device.modelData.id
                error: root.revoke.revokeErrorId === device.modelData.id ? (root.revoke.revokeError ?? "") : ""
                onCancelled: root.armedSession = ""
                onConfirmed: Extras.revokeSession(device.modelData.id)
            }
        }
    }

    // A fact: the label, the value in the number face.
    component InfoRow: Item {
        id: info
        property string text
        property string value
        width: parent ? parent.width : 0
        height: 48
        Accessible.role: Accessible.StaticText
        Accessible.name: text + ", " + value
        Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: info.text
            color: Theme.ink1
            font: Theme.type.setting
        }
        Text {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: info.value
            color: Theme.ink2
            font: Theme.type.num
        }
    }

    // A page on the web, opened in the browser.
    component LinkRow: Item {
        id: link
        property string text
        property string url
        width: parent ? parent.width : 0
        height: 48
        Accessible.role: Accessible.Link
        Accessible.name: text
        Accessible.onPressAction: Qt.openUrlExternally(link.url)
        Rectangle {
            anchors.fill: parent
            anchors.leftMargin: -8
            anchors.rightMargin: -8
            radius: Theme.radius
            color: linkTap.pressed ? Theme.tint2 : "transparent"
            Behavior on color { ColorAnimation { duration: Theme.dur1 } }
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: link.text
            color: Theme.ink1
            font: Theme.type.setting
        }
        Icon {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            name: "externalLink"
            size: Theme.iconMd
            color: Theme.ink3
        }
        TapHandler {
            id: linkTap
            onTapped: {
                Native.haptic(Native.Tick)
                Qt.openUrlExternally(link.url)
            }
        }
    }
}
