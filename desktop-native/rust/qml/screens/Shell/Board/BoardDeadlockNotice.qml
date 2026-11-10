import QtQuick
import com.yap.native

// The owner-only states (Deadlock.tsx NotLinkedPlate / OwnerNotice): not
// linked ("Link Steam"), linked with no matches yet, and data deleted at
// the source. Everyone else never gets these plates. The Link Steam key
// raises the plate's linkSteamRequested(); the caller opens Connections.
Item {
    id: view

    required property var dl
    readonly property string kind: dl.view
    readonly property string persona: dl.board?.persona ?? ""

    Column {
        width: Math.min(view.width, 46 * 7)
        y: Math.max(0, Math.round((view.height - height) / 2))
        spacing: 6

        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: view.kind === "not_linked" ? qsTr("Show your Deadlock stats")
                : view.kind === "empty" ? qsTr("No matches yet")
                : qsTr("Stats turned off at the source")
            wrapMode: Text.Wrap
            maximumLineCount: 2
            elide: Text.ElideRight
            color: Theme.ink1
            font: view.dl.titleFont
            Accessible.role: Accessible.Heading
            Accessible.name: text
        }
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: view.kind === "not_linked"
                ? qsTr("Link the Steam account you play on. Friends see your rank, heroes and live matches.")
                : view.kind === "empty"
                ? (view.persona.length > 0
                   ? qsTr("Play a match on %1 and it shows here within the hour.").arg(view.persona)
                   : qsTr("Play a match and it shows here within the hour."))
                : qsTr("This Steam account asked deadlock-api.com to delete its data, so there is nothing to show. Live matches still work on desktop.")
            wrapMode: Text.Wrap
            maximumLineCount: view.dl.size === "M" ? 6 : 3
            elide: Text.ElideRight
            lineHeight: 18
            lineHeightMode: Text.FixedHeight
            color: Theme.ink2
            font: Theme.type.description
        }
        Item { width: 1; height: 4; visible: link.visible }
        KitButton {
            id: link
            visible: view.kind === "not_linked"
            variant: "secondary"
            size: "sm"
            iconName: "link"
            text: qsTr("Link Steam")
            onClicked: view.dl.linkSteamRequested()
        }
    }
}
