import QtQuick
import com.yap.native

// Settings > Yap Loud (LoudPanel.tsx): the plan in force, with its Founder
// or Loud badge and "Founder · lifetime" (else the plan's name), then the
// three plans (TierCards). Opening the page asks whether billing is on
// (`GET /api/billing/status`): until it answers 200 the Upgrade key stays
// off with "Coming soon". The uploads line on each card is that plan's
// limit as the server enforces it (500 MB with bigUploads, 25 MB free).
Column {
    id: root

    readonly property var plan: Settings.plan.length > 0 ? JSON.parse(Settings.plan) : null

    spacing: 40
    Component.onCompleted: Settings.loadBilling()

    SettingsCard {
        width: parent.width
        sectionId: "your-plan"
        SettingsRow {
            rowId: "current-plan"
            layout: "inline"
            description: root.plan ? root.plan.description : ""
            Row {
                spacing: 8
                UserBadges {
                    anchors.verticalCenter: parent.verticalCenter
                    badges: root.plan ? root.plan.badges : ""
                    size: "md"
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: root.plan ? root.plan.label : qsTr("Loading…")
                    width: Math.min(implicitWidth, 320)
                    elide: Text.ElideRight
                    color: Theme.ink2
                    font: Theme.type.row
                }
            }
        }
    }

    SettingsCard {
        width: parent.width
        sectionId: "plans"
        SettingsRow {
            rowId: "tier-cards"
            layout: "block"
            hideLabel: true
            TierCards {
                width: parent.width
                plan: root.plan
            }
        }
    }
}
