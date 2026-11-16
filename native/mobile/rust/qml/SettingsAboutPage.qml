import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Settings > About: the version, the licence and privacy pages on the web,
// and the open source licences the APK carries (LicensesPage, over this
// page).
SettingsFrame {
    id: root

    title: qsTr("About")

    overlays: Loader {
        id: licensesPage
        anchors.fill: parent
        active: false
        sourceComponent: LicensesPage {}
        function load() {
            active = true
            return item
        }
    }

    SettingsSection {
        number: 1
        title: qsTr("Yap")
        Item {
            readonly property string version: Qt.application.version.length > 0 ? Qt.application.version : Settings.version
            width: parent.width
            height: 48
            Accessible.role: Accessible.StaticText
            Accessible.name: qsTr("Version") + ", " + version
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: qsTr("Version")
                color: Theme.ink1
                font: Theme.type.setting
            }
            Text {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: parent.version
                color: Theme.ink2
                font: Theme.type.num
            }
        }
        SettingsActionRow {
            text: qsTr("Licence")
            url: "https://github.com/115jon/yap/blob/main/LICENSE"
        }
        SettingsActionRow {
            text: qsTr("Privacy")
            // The site's page (PRIVACY.md), on the core's web origin.
            url: Admin.webOrigin + "/privacy"
        }
        SettingsActionRow {
            text: qsTr("Open source licences")
            onOpened: licensesPage.load().open()
        }
    }
}
