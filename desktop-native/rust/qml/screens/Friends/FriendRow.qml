import QtQuick
import com.yap.native

// One person in the Friends view (FriendsView.tsx `renderRow`, the member
// and DM row anatomy): 32 high, 40 with a friend's custom status, px-2,
// gap 10, a 24 px avatar with its presence LED (none when blocked) that
// opens the profile, the name at 14/500. A press anywhere else opens the
// DM; right-click opens the row menu. Incoming requests always show Accept
// and Ignore; friends and sent requests keep Message and Remove / Cancel
// in a hover overlay that the name fades under.
Rectangle {
    id: row

    required property var person
    // The person's action is running (its keys go quiet).
    property bool busy: false

    signal messageRequested()
    signal profileRequested(Item anchor)
    signal menuRequested(point at)
    signal acceptRequested()
    signal removeRequested()

    readonly property bool incoming: person.kind === "incoming"
    readonly property bool hoverActions: person.kind === "friend" || person.kind === "sent"
    // The keys stay in the tab order (transparent until hovered or focused).
    readonly property bool overlay: hoverActions && (hover.hovered || messageKey.activeFocus || removeKey.activeFocus)
    readonly property string removeLabel: person.kind === "friend" ? qsTr("Remove friend")
        : person.kind === "incoming" ? qsTr("Ignore") : qsTr("Cancel")

    height: person.statusLine.length > 0 ? 40 : Theme.treeRow
    radius: Theme.radius
    color: hover.hovered ? Theme.tint1 : "transparent"
    Behavior on color { ColorAnimation { duration: Theme.dur1 } }

    Accessible.role: Accessible.Button
    Accessible.name: qsTr("Message %1").arg(person.name)

    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: row.messageRequested() }
    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: eventPoint => row.menuRequested(eventPoint.position)
    }

    PresenceAvatar {
        id: face
        x: 8
        anchors.verticalCenter: parent.verticalCenter
        size: 24
        initials: row.person.initials
        source: row.person.avatar
        presence: row.person.kind === "blocked" ? "" : row.person.presence
        ground: Theme.ply0
        overhang: 4
        // A MouseArea, so the press stops here instead of opening the DM.
        MouseArea {
            anchors.fill: parent
            anchors.margins: -2
            cursorShape: Qt.PointingHandCursor
            onClicked: row.profileRequested(face)
        }
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("View %1's profile").arg(row.person.name)
        Accessible.ignored: false
    }

    Column {
        anchors.left: face.right
        anchors.leftMargin: 10
        anchors.right: requestKeys.visible ? requestKeys.left : parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        opacity: row.overlay ? 0.35 : 1
        Behavior on opacity { OpacityAnimator { duration: Theme.dur1 } }
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: row.person.name
            elide: Text.ElideRight
            color: hover.hovered ? Theme.ink1 : Theme.ink2
            font: Theme.type.row
        }
        Text {
            width: parent.width
            visible: row.person.statusLine.length > 0
            textFormat: Text.PlainText
            text: row.person.statusLine
            elide: Text.ElideRight
            color: Theme.ink3
            font: Theme.type.caption
        }
    }

    Row {
        id: requestKeys
        visible: row.incoming
        anchors.right: parent.right
        anchors.rightMargin: 4
        anchors.verticalCenter: parent.verticalCenter
        spacing: 4
        KitButton {
            size: "sm"
            variant: "secondary"
            text: qsTr("Accept")
            loading: row.busy
            enabled: !row.busy
            Accessible.name: qsTr("Accept %1").arg(row.person.name)
            onClicked: row.acceptRequested()
        }
        KitButton {
            size: "sm"
            variant: "ghost"
            text: qsTr("Ignore")
            enabled: !row.busy
            Accessible.name: qsTr("Ignore %1").arg(row.person.name)
            onClicked: row.removeRequested()
        }
    }

    Row {
        id: actions
        visible: row.hoverActions
        opacity: row.overlay ? 1 : 0
        anchors.right: parent.right
        anchors.rightMargin: 4
        anchors.verticalCenter: parent.verticalCenter
        spacing: 2
        IconKey {
            id: messageKey
            visible: row.person.kind === "friend"
            size: 24
            iconName: "chat"
            tip: qsTr("Message")
            onClicked: row.messageRequested()
        }
        IconKey {
            id: removeKey
            size: 24
            iconName: "close"
            tip: row.removeLabel
            enabled: !row.busy
            onClicked: row.removeRequested()
        }
    }
}
