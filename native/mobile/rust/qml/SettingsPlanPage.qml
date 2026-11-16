import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Settings > Yap Loud (the desktop's LoudPanel): the plan in force and the
// three plans (the core's plan_json). No payments yet, so Upgrade stays
// off with "Coming soon" until billing answers.
SettingsFrame {
    id: root

    readonly property var plan: {
        try {
            return Settings.plan.length > 0 ? JSON.parse(Settings.plan) : null
        } catch (e) {
            return null
        }
    }

    title: qsTr("Yap Loud")

    Component.onCompleted: Settings.loadBilling()

    SettingsSection {
        number: 1
        title: qsTr("Your plan")
        Column {
            width: parent.width
            bottomPadding: 16
            spacing: 2
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: root.plan ? root.plan.label : qsTr("Loading…")
                wrapMode: Text.Wrap
                color: Theme.ink1
                font: Theme.type.setting
            }
            Text {
                width: parent.width
                visible: text.length > 0
                textFormat: Text.PlainText
                text: root.plan ? root.plan.description : ""
                wrapMode: Text.Wrap
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
    }

    SettingsSection {
        number: 2
        title: qsTr("Plans")
        TierCards {
            width: parent.width
            plan: root.plan
        }
    }
}
