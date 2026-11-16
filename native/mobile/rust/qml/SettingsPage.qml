pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Settings: a hub of categories, each its own page (Router), grouped as the
// web's settings are (SETTINGS_NAV_GROUPS: You, Calls, App). Your card on
// top opens Profile; the search field finds a setting by name and opens
// its page; Back clears a search before it leaves. Opened from the account
// panel.
SettingsFrame {
    id: root

    // Each category: its page, glyph, title and one line of what it holds.
    readonly property var pages: ({
        SettingsProfilePage: { icon: "profile", title: qsTr("Profile") },
        SettingsAccountPage: { icon: "account", title: qsTr("Account") },
        ConnectionsPage: { icon: "connections", title: qsTr("Connections") },
        SettingsPrivacyPage: { icon: "privacy", title: qsTr("Privacy & safety") },
        SettingsDevicesPage: { icon: "sessions", title: qsTr("Devices") },
        SettingsPlanPage: { icon: "sparkle", title: qsTr("Yap Loud") },
        SettingsVoicePage: { icon: "mic", title: qsTr("Voice & video") },
        SettingsSoundboardPage: { icon: "soundboard", title: qsTr("Soundboard") },
        SettingsAppearancePage: { icon: "appearance", title: qsTr("Appearance") },
        SettingsNotificationsPage: { icon: "bell", title: qsTr("Notifications") },
        SettingsAboutPage: { icon: "info", title: qsTr("About") }
    })
    // What search finds: each setting's name, the words someone might type
    // for it, and the page it is on.
    readonly property var index: [
        { page: "SettingsProfilePage", label: qsTr("Edit profile"), words: "avatar banner bio name picture" },
        { page: "SettingsProfilePage", label: qsTr("Status"), words: "online idle away do not disturb dnd invisible presence" },
        { page: "SettingsProfilePage", label: qsTr("Custom status"), words: "status message" },
        { page: "SettingsAccountPage", label: qsTr("Username"), words: "handle sign in" },
        { page: "SettingsAccountPage", label: qsTr("Sign out"), words: "log out logout" },
        { page: "SettingsAccountPage", label: qsTr("Delete account"), words: "remove close account" },
        { page: "ConnectionsPage", label: qsTr("Steam"), words: "connections link linked accounts deadlock" },
        { page: "ConnectionsPage", label: qsTr("Show on profile"), words: "connections steam linked accounts" },
        { page: "SettingsPrivacyPage", label: qsTr("Share my activity"), words: "playing game activity status" },
        { page: "SettingsPrivacyPage", label: qsTr("Who can DM you"), words: "direct messages dm calls block" },
        { page: "SettingsPrivacyPage", label: qsTr("Media filter"), words: "gif sticker safe search nsfw sensitive" },
        { page: "SettingsDevicesPage", label: qsTr("Signed-in devices"), words: "sessions sign out other devices phone computer" },
        { page: "SettingsPlanPage", label: qsTr("Your plan"), words: "loud founder billing upgrade subscription tier" },
        { page: "SettingsVoicePage", label: qsTr("Output"), words: "speaker earpiece phone headset bluetooth headphones audio route device" },
        { page: "SettingsVoicePage", label: qsTr("Input profile"), words: "voice isolation studio custom microphone mic" },
        { page: "SettingsVoicePage", label: qsTr("Noise suppression"), words: "background noise mic microphone" },
        { page: "SettingsVoicePage", label: qsTr("Echo cancellation"), words: "echo feedback mic microphone" },
        { page: "SettingsVoicePage", label: qsTr("Automatic gain control"), words: "agc volume level mic microphone" },
        { page: "SettingsVoicePage", label: qsTr("High fidelity"), words: "music quality mic microphone" },
        { page: "SettingsVoicePage", label: qsTr("Input sensitivity"), words: "voice activity threshold level meter test mic microphone" },
        { page: "SettingsVoicePage", label: qsTr("Camera"), words: "front back video selfie" },
        { page: "SettingsVoicePage", label: qsTr("Data saver"), words: "camera video quality mobile data" },
        { page: "SettingsSoundboardPage", label: qsTr("Soundboard volume"), words: "sounds loud" },
        { page: "SettingsAppearancePage", label: qsTr("Theme"), words: "dark light colour color mode match system" },
        { page: "SettingsAppearancePage", label: qsTr("Use on all my devices"), words: "theme sync" },
        { page: "SettingsAppearancePage", label: qsTr("Text size"), words: "font bigger smaller zoom scale" },
        { page: "SettingsNotificationsPage", label: qsTr("Push notifications"), words: "alerts notify" },
        { page: "SettingsNotificationsPage", label: qsTr("Sound effects"), words: "sounds volume chime mute" },
        { page: "SettingsNotificationsPage", label: qsTr("Notification sounds"), words: "mentions replies dm chime" },
        { page: "SettingsNotificationsPage", label: qsTr("Sound volume"), words: "sounds loud" },
        { page: "SettingsNotificationsPage", label: qsTr("Voice join and leave"), words: "sounds tone voice channel" },
        { page: "SettingsNotificationsPage", label: qsTr("Connect and disconnect"), words: "sounds chime voice channel" },
        { page: "SettingsNotificationsPage", label: qsTr("Mute and deafen"), words: "sounds click" },
        { page: "SettingsNotificationsPage", label: qsTr("Calls"), words: "sounds ringback ringtone dm call" },
        { page: "SettingsNotificationsPage", label: qsTr("Screen share"), words: "sounds stream tone" },
        { page: "SettingsNotificationsPage", label: qsTr("Test notification sound"), words: "sound play" },
        { page: "SettingsAboutPage", label: qsTr("Version"), words: "about update build" },
        { page: "SettingsAboutPage", label: qsTr("Open source licences"), words: "licenses notices" },
        { page: "SettingsAboutPage", label: qsTr("Privacy"), words: "policy" }
    ]
    readonly property bool hasSoundboard: Apps.soundboardVolume() >= 0
    // The word in progress counts: the IME has not committed it.
    readonly property string query: (searchField.text + searchField.preeditText).trim().toLowerCase()
    // Every word of the query, each in the setting's name, its page's name
    // or its keywords.
    readonly property var results: {
        if (query.length === 0)
            return []
        const words = query.split(/\s+/)
        return index.filter(entry => {
            if (entry.page === "SettingsSoundboardPage" && !hasSoundboard)
                return false
            const haystack = (entry.label + " " + pages[entry.page].title + " " + entry.words).toLowerCase()
            return words.every(word => haystack.includes(word))
        })
    }
    readonly property string status: Session.meStatus.length > 0 ? Session.meStatus : "online"
    readonly property string themeName: Settings.theme === "system" ? qsTr("Match system") : Theme.themeLabel(Theme.themeId)

    // A search takes Back (and its predictive gesture) to clear it, while
    // this page is on top.
    // Activating too: a second Back while the page over this one leaves
    // still clears first.
    readonly property bool holdsLayer: query.length > 0
        && (StackView.status === StackView.Active || StackView.status === StackView.Activating)
    onHoldsLayerChanged: holdsLayer ? Native.pushBackLayer(root) : Native.popBackLayer(root)
    Component.onDestruction: Native.popBackLayer(root)
    function backProgressed(progress) {}
    function backCancelled() {}
    function backInvoked() {
        clearSearch()
    }
    function clearSearch() {
        searchField.text = ""
        searchField.focus = false
        Qt.inputMethod.hide()
    }
    // The header's Back too.
    beforeBack: () => {
        if (query.length === 0)
            return false
        clearSearch()
        return true
    }

    function open(page) {
        Qt.inputMethod.hide()
        Router.open(page, {}, { key: page })
    }

    title: qsTr("Settings")

    // Search, as Android's own Settings has it on top.
    Item {
        width: parent.width
        height: searchField.height
        MobileField {
            id: searchField
            width: parent.width
            leftPadding: 44
            rightPadding: clearKey.visible ? 48 : 14
            placeholderText: qsTr("Search settings")
            inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText
            EnterKey.type: Qt.EnterKeySearch
            Accessible.name: qsTr("Search settings")
            onAccepted: Qt.inputMethod.hide()
        }
        Icon {
            x: 14
            anchors.verticalCenter: parent.verticalCenter
            name: "search"
            size: Theme.iconMd
            color: Theme.ink3
        }
        IconKey {
            id: clearKey
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            visible: root.query.length > 0
            iconName: "close"
            tip: qsTr("Clear search")
            onClicked: {
                Qt.inputMethod.reset()
                searchField.text = ""
            }
        }
    }

    // Search results: the setting, the page it is on.
    Column {
        visible: root.query.length > 0
        width: parent.width
        Repeater {
            model: root.results
            delegate: AdminRow {
                required property var modelData
                iconName: root.pages[modelData.page].icon
                text: modelData.label
                hint: root.pages[modelData.page].title
                onOpened: root.open(modelData.page)
            }
        }
        Text {
            visible: root.results.length === 0
            width: parent.width
            topPadding: 24
            horizontalAlignment: Text.AlignHCenter
            textFormat: Text.PlainText
            text: qsTr("No settings match “%1”.").arg(root.query)
            wrapMode: Text.Wrap
            color: Theme.ink3
            font: Theme.type.description
        }
    }

    // You: your card (Profile), then the account's pages.
    Rectangle {
        visible: root.query.length === 0
        width: parent.width
        height: card.height + 32
        radius: Theme.radius
        color: cardTap.pressed ? Theme.tint2 : Theme.ply1
        border.width: 1
        border.color: Theme.line
        Behavior on color { ColorAnimation { duration: Theme.dur1 } }
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Profile, %1").arg(Session.meName)
        Accessible.onPressAction: root.open("SettingsProfilePage")
        Row {
            id: card
            x: 16
            y: 16
            width: parent.width - 32
            spacing: 16
            PresenceAvatar {
                id: face
                anchors.verticalCenter: parent.verticalCenter
                size: 56
                initials: Session.meInitials.length > 0 ? Session.meInitials : "?"
                source: Session.meAvatar
                deco: Session.meDeco
                presence: root.status
                ground: Theme.ply1
            }
            Column {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - face.width - chevron.width - 2 * parent.spacing
                spacing: 2
                DisplayName {
                    width: parent.width
                    text: Session.meName
                    nameStyle: Session.meNameStyle
                    color: Theme.ink1
                    font: Theme.type.sectionTitle
                    ground: Theme.ply1
                }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: qsTr("@%1 · Profile and status").arg(Session.meUsername)
                    elide: Text.ElideRight
                    color: Theme.ink2
                    font: Theme.type.description
                }
            }
            Icon {
                id: chevron
                anchors.verticalCenter: parent.verticalCenter
                name: "chevronRight"
                size: Theme.iconMd
                color: Theme.ink3
            }
        }
        TapHandler {
            id: cardTap
            onTapped: {
                Native.haptic(Native.Tick)
                root.open("SettingsProfilePage")
            }
        }
    }

    AdminSection {
        visible: root.query.length === 0
        title: qsTr("You")
        HubRow {
            page: "SettingsAccountPage"
            hint: qsTr("Sign out, delete account")
        }
        HubRow {
            page: "ConnectionsPage"
            hint: qsTr("Linked accounts, such as Steam")
        }
        HubRow {
            page: "SettingsPrivacyPage"
            hint: qsTr("Activity, DMs, media filter")
        }
        HubRow {
            page: "SettingsDevicesPage"
            hint: qsTr("Where you're signed in")
        }
        HubRow {
            page: "SettingsPlanPage"
            hint: qsTr("Your plan")
        }
    }

    AdminSection {
        visible: root.query.length === 0
        title: qsTr("Calls")
        HubRow {
            page: "SettingsVoicePage"
            hint: qsTr("Output, mic and camera")
        }
        HubRow {
            visible: root.hasSoundboard
            page: "SettingsSoundboardPage"
            hint: qsTr("Playback volume")
        }
    }

    AdminSection {
        visible: root.query.length === 0
        title: qsTr("App")
        HubRow {
            page: "SettingsAppearancePage"
            hint: qsTr("%1 · Text size").arg(root.themeName)
        }
        HubRow {
            page: "SettingsNotificationsPage"
            hint: Native.notificationsEnabled ? qsTr("Push on · Sounds") : qsTr("Push off · Sounds")
        }
        HubRow {
            page: "SettingsAboutPage"
            hint: qsTr("Version %1, licences").arg(Qt.application.version.length > 0 ? Qt.application.version : Settings.version)
        }
    }

    // A category: its glyph, title and line, opening its page.
    component HubRow: AdminRow {
        required property string page
        iconName: root.pages[page].icon
        text: root.pages[page].title
        onOpened: root.open(page)
    }
}
