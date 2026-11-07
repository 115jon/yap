pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import com.yap.native

// Home with no conversation open (FriendsView.tsx): the 48 px header with
// the members glyph, "Friends", the tabs (Online, All, Pending with the
// incoming count, Blocked; in their own strip under the header below 560
// px) and Add friend, which opens the form under it. Then the tab's rows,
// capped at 720, under a branch label with the count (Pending splits into
// Incoming and Sent), the loading rows while the list loads empty, or the
// tab's empty state. The row menu: Profile (the full profile), Message,
// Start a call (its DM opens first), then Remove, Ignore or Cancel, which
// ask first.
Rectangle {
    id: root

    // FriendsView.tsx `initialTab`.
    property string tab: "online"
    property bool adding: false

    readonly property var model: JSON.parse(Friends.state)
    readonly property bool narrow: width < 560
    // The tab's rows. Every state change parses `model` afresh, and a new
    // array would rebuild every row in the Repeaters; this one is replaced
    // only when the rows shown differ (as SearchPanel.parse does).
    property var branches: []
    property string branchesKey: ""
    readonly property int shown: branches.reduce((n, b) => n + b.rows.length, 0)

    function syncBranches() {
        const next = tab === "pending"
            ? [{ id: "incoming", label: qsTr("Incoming"), rows: model.incoming ?? [] },
               { id: "sent", label: qsTr("Sent"), rows: model.sent ?? [] }].filter(b => b.rows.length > 0)
            : [{ id: tab, label: listLabel(tab), rows: rowsOf(tab) }]
        const key = JSON.stringify(next)
        if (key !== branchesKey) {
            branchesKey = key
            branches = next
        }
    }
    onModelChanged: syncBranches()
    onTabChanged: syncBranches()

    function rowsOf(t) {
        return t === "online" ? model.online ?? []
            : t === "all" ? model.all ?? []
            : t === "blocked" ? model.blocked ?? [] : []
    }
    function listLabel(t) {
        return t === "online" ? qsTr("Online")
            : t === "all" ? qsTr("All friends")
            : t === "blocked" ? qsTr("Blocked") : qsTr("Pending")
    }
    function actOf(userId) {
        return model.acts?.[userId] ?? { busy: "", error: "", failed: "", done: 0 }
    }
    function toggleAdd() {
        adding = !adding
    }

    color: Theme.ply0
    Component.onCompleted: {
        syncBranches()
        Friends.load()
    }

    Item {
        id: header
        width: parent.width
        height: Theme.header

        Row {
            id: title
            x: 16
            height: parent.height
            spacing: 8
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: "members"
                size: Theme.iconLg
                color: Theme.ink3
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Friends")
                color: Theme.ink1
                font: Theme.type.pageTitle
                Accessible.role: Accessible.Heading
            }
            Item {
                visible: !root.narrow
                anchors.verticalCenter: parent.verticalCenter
                // mx-2 inside the row's gap-2: 16 either side.
                width: 1 + 16
                height: 20
                Rectangle {
                    anchors.centerIn: parent
                    width: 1
                    height: parent.height
                    color: Theme.line
                }
            }
            Loader {
                active: !root.narrow
                height: parent.height
                sourceComponent: tabs
            }
        }

        KitButton {
            anchors.right: parent.right
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            size: "sm"
            // The page's one primary action; it steps back while its form
            // is open, so Send request is the primary then.
            variant: root.adding ? "secondary" : "primary"
            iconName: "invite"
            text: qsTr("Add friend")
            Accessible.checkable: true
            Accessible.checked: root.adding
            onClicked: root.toggleAdd()
        }

        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
    }

    Column {
        id: top
        anchors.top: header.bottom
        width: parent.width

        Loader {
            active: root.narrow
            visible: active
            width: parent.width
            height: active ? 44 + 1 : 0
            sourceComponent: Item {
                Loader {
                    x: 16
                    height: 44
                    sourceComponent: tabs
                }
                Rectangle {
                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: 1
                    color: Theme.line
                }
            }
        }

        Loader {
            active: root.adding
            visible: active
            width: parent.width
            sourceComponent: AddFriendForm {
                status: root.model.add ?? { kind: "", message: "", sent: 0 }
            }
        }
    }

    Flickable {
        id: scroller
        anchors.top: top.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        contentHeight: list.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds

        Column {
            id: list
            width: Math.min(720, scroller.width)
            leftPadding: 8
            rightPadding: 8
            bottomPadding: 16

            Loader {
                active: root.model.loading && root.shown === 0
                visible: active
                width: list.width - 16
                sourceComponent: LoadingFriends {}
            }

            Loader {
                active: !root.model.loading && root.shown === 0
                visible: active
                width: list.width - 16
                sourceComponent: emptyState
            }

            Repeater {
                model: root.shown > 0 ? root.branches : []
                delegate: Column {
                    id: branch
                    required property var modelData
                    width: list.width - 16
                    topPadding: 16
                    spacing: 2
                    Item {
                        width: parent.width
                        height: 24
                        Accessible.role: Accessible.Heading
                        Accessible.name: branch.modelData.label
                        Row {
                            x: 8
                            height: parent.height
                            spacing: 8
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                textFormat: Text.PlainText
                                text: branch.modelData.label
                                color: Theme.ink3
                                font: Theme.type.silk
                            }
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                textFormat: Text.PlainText
                                text: branch.modelData.rows.length
                                color: Theme.ink3
                                font: Theme.type.numSm
                            }
                        }
                    }
                    Repeater {
                        model: branch.modelData.rows
                        delegate: FriendRow {
                            id: friend
                            required property var modelData
                            person: modelData
                            width: branch.width
                            busy: root.actOf(modelData.userId).busy.length > 0
                            onMessageRequested: Nav.openDm(friend.person.userId)
                            onProfileRequested: anchor => ShellState.showProfile(friend.person.userId, anchor, "right")
                            onMenuRequested: at => rowMenu.openFor(friend.person, friend, at)
                            onAcceptRequested: Friends.act("accept", friend.person.userId, friend.person.username, friend.person.name)
                            onRemoveRequested: removeDialog.ask(friend.person)
                        }
                    }
                }
            }
        }
    }

    Component {
        id: tabs
        Row {
            spacing: 24
            Accessible.role: Accessible.PageTabList
            Accessible.name: qsTr("Friends")
            FriendsTab { value: "online"; text: qsTr("Online") }
            FriendsTab { value: "all"; text: qsTr("All") }
            // Incoming requests need you, so they earn the count badge.
            FriendsTab { value: "pending"; text: qsTr("Pending"); count: Friends.pending }
            FriendsTab { value: "blocked"; text: qsTr("Blocked") }
        }
    }

    Component {
        id: emptyState
        // ui/empty-state.tsx md with FriendsView's py-16, the DitherGlyph
        // "empty" slot (3 px cells), 16 below it, the title 20/700 and the
        // line 4 apart.
        Column {
            topPadding: 64
            bottomPadding: 64
            spacing: 4
            Item {
                width: parent.width
                height: 60 + 12
                Image {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: 26 * 3
                    height: 20 * 3
                    smooth: false
                    source: "image://dither/empty/" + Theme.hex(Theme.ink3) + "/" + Theme.hex(Theme.accentInk)
                    Accessible.ignored: true
                }
            }
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                textFormat: Text.PlainText
                text: root.tab === "online" ? qsTr("No friends online")
                    : root.tab === "all" ? qsTr("No friends yet")
                    : root.tab === "pending" ? qsTr("No pending requests")
                    : qsTr("No blocked users")
                color: Theme.ink1
                font: Theme.type.sectionTitle
                Accessible.role: Accessible.Heading
            }
            Text {
                visible: root.tab === "all"
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                text: qsTr("Add a friend by their username to start a conversation.")
                color: Theme.ink2
                font: Theme.type.description
            }
            Item {
                visible: root.tab === "all" && !root.adding
                width: parent.width
                height: 8 + addKey.height
                KitButton {
                    id: addKey
                    y: 8
                    anchors.horizontalCenter: parent.horizontalCenter
                    variant: "secondary"
                    text: qsTr("Add friend")
                    onClicked: root.toggleAdd()
                }
            }
        }
    }

    // Profile, Message, then Remove / Ignore / Cancel (not for blocked).
    MenuPanel {
        id: rowMenu
        property var person: ({ userId: "", name: "", kind: "" })
        property Item anchorItem: null
        function openFor(who, anchor, at) {
            person = who
            anchorItem = anchor
            popup(anchor, at.x, at.y)
        }
        implicitWidth: 220
        MenuRow {
            text: qsTr("Profile")
            glyph: "members"
            onTriggered: ShellState.showFullProfile(rowMenu.person.userId, rowMenu.anchorItem)
        }
        MenuRow {
            text: qsTr("Message")
            glyph: "chat"
            onTriggered: Nav.openDm(rowMenu.person.userId)
        }
        MenuRow {
            readonly property var call: JSON.parse(Calls.view)
            visible: !((call.status === "ringing_outgoing" || call.joined === true) && call.remote && call.remote.id === rowMenu.person.userId)
            height: visible ? implicitHeight : 0
            text: qsTr("Start a call")
            glyph: "call"
            onTriggered: ShellState.startCallWithRequested(rowMenu.person.userId, rowMenu.person.name)
        }
        // groupMenuItems: the danger row last, behind a rule.
        MenuRule { visible: rowMenu.person.kind !== "blocked"; height: visible ? implicitHeight : 0 }
        MenuRow {
            visible: rowMenu.person.kind !== "blocked"
            height: visible ? implicitHeight : 0
            text: rowMenu.person.kind === "friend" ? qsTr("Remove friend")
                : rowMenu.person.kind === "incoming" ? qsTr("Ignore") : qsTr("Cancel")
            glyph: "close"
            danger: true
            onTriggered: removeDialog.ask(rowMenu.person)
        }
    }

    RemoveFriendDialog {
        id: removeDialog
        act: root.actOf(removeDialog.userId)
    }

    component FriendsTab: T.AbstractButton {
        id: tabKey
        property string value
        property int count: 0
        readonly property bool on: root.tab === value
        implicitWidth: implicitContentWidth
        height: parent ? parent.height : 44
        focusPolicy: Qt.StrongFocus
        Accessible.role: Accessible.PageTab
        Accessible.name: count > 0 ? qsTr("%1, %n incoming request(s)", "", count).arg(text) : text
        Accessible.selected: on
        onClicked: root.tab = value
        contentItem: Row {
            spacing: 6
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: tabKey.text
                color: tabKey.on || tabKey.hovered ? Theme.ink1 : Theme.ink3
                Behavior on color { ColorAnimation { duration: Theme.dur1 } }
                font: Theme.type.control
            }
            CountBadge {
                anchors.verticalCenter: parent.verticalCenter
                count: tabKey.count
            }
        }
        background: Item {
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 2
                color: tabKey.on ? Theme.ink1 : "transparent"
            }
            Rectangle {
                anchors.fill: parent
                anchors.margins: -2
                visible: tabKey.visualFocus
                color: "transparent"
                radius: Theme.radius
                border.width: 2
                border.color: Theme.focus
            }
        }
        HoverHandler { cursorShape: Qt.PointingHandCursor }
    }
}
