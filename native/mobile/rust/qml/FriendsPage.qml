pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Friends (the desktop's FriendsView, phone layout), a page over the pager
// on the shared Friends object: the header with Add friend, the four tabs
// (Online, All, Pending with the incoming count, Blocked) on a sliding
// rule, the add form under them, then the tab's people. A friend's row
// opens the DM; anyone else's opens their profile. Requests carry Accept
// and Ignore or Cancel; a friend's more key opens Profile, Remove friend
// and Block in the row; a blocked row carries Unblock. Remove, Ignore,
// Cancel and Block ask first, in place.
Rectangle {
    id: root

    readonly property bool sheet: false
    property real backProgress: 0

    signal closeRequested()
    // The DM with them (made now if they never talked), opened by Main.
    signal messageRequested(string userId)
    signal profileRequested(string userId)

    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: SafeArea.margins.bottom

    property string tab: "online"
    property bool adding: false
    // The friend whose actions are open, and the step being confirmed:
    // `{userId, action, done}` (`done`: the person's finished count when
    // asked, so the strip closes once the action lands).
    property string openId: ""
    property var confirming: null

    readonly property var model: {
        try {
            return JSON.parse(Friends.state)
        } catch (e) {
            return {}
        }
    }
    // The tab's rows, replaced only when the rows shown differ (a new array
    // would rebuild every row; as the desktop's FriendsView).
    property var branches: []
    property string branchesKey: ""
    readonly property int shown: branches.reduce((n, b) => n + b.rows.length, 0)

    function syncBranches() {
        const next = tab === "pending"
            ? [{ id: "incoming", label: qsTr("Incoming"), rows: model.incoming ?? [] },
               { id: "sent", label: qsTr("Sent"), rows: model.sent ?? [] }].filter(b => b.rows.length > 0)
            : [{ id: tab, label: listLabel(tab), rows: tab === "online" ? model.online ?? []
                : tab === "all" ? model.all ?? [] : model.blocked ?? [] }]
        const key = JSON.stringify(next)
        if (key !== branchesKey) {
            branchesKey = key
            branches = next
        }
        // A confirmed step that landed closes its strip.
        if (confirming && actOf(confirming.userId).done > confirming.done) {
            confirming = null
            openId = ""
        }
    }
    onModelChanged: syncBranches()
    onTabChanged: {
        openId = ""
        confirming = null
        syncBranches()
    }

    function listLabel(t) {
        return t === "online" ? qsTr("Online")
            : t === "all" ? qsTr("All friends")
            : t === "blocked" ? qsTr("Blocked") : qsTr("Pending")
    }
    function actOf(userId) {
        return model.acts?.[userId] ?? { busy: "", error: "", failed: "", done: 0 }
    }
    function ask(person, action) {
        // A new step starts clean (not with the last attempt's failure).
        Friends.clear(person.userId)
        confirming = { userId: person.userId, action: action, done: actOf(person.userId).done }
    }
    function act(person, action) {
        Friends.act(action, person.userId, person.username, person.name)
    }

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

    Component.onCompleted: {
        // Requests wait for you: the page opens on them.
        if (Friends.pending > 0)
            tab = "pending"
        syncBranches()
        Friends.load()
    }

    PageHeader {
        id: header
        z: 1
        title: qsTr("Friends")
        topInset: root.topInset
        leftInset: root.leftInset
        rightInset: root.rightInset
        onBackRequested: root.closeRequested()
        IconKey {
            iconName: root.adding ? "close" : "invite"
            tone: root.adding ? "toggled" : "ghost"
            tip: root.adding ? qsTr("Close Add friend") : qsTr("Add friend")
            onClicked: root.adding = !root.adding
        }
    }

    Column {
        id: top
        anchors.top: header.bottom
        width: parent.width
        z: 1

        // The tabs: equal cells, the 2 px rule sliding to the chosen one.
        Rectangle {
            width: parent.width
            height: 48
            color: Theme.ply0
            Row {
                id: tabs
                x: 8 + root.leftInset
                width: parent.width - 16 - root.leftInset - root.rightInset
                height: parent.height
                Accessible.role: Accessible.PageTabList
                Accessible.name: qsTr("Friends")
                FriendsTab { value: "online"; text: qsTr("Online") }
                FriendsTab { value: "all"; text: qsTr("All") }
                FriendsTab { value: "pending"; text: qsTr("Pending"); count: Friends.pending }
                FriendsTab { value: "blocked"; text: qsTr("Blocked") }
            }
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 1
                color: Theme.line
            }
            TabRule { row: tabs }
        }

        Loader {
            width: parent.width
            active: root.adding
            visible: active
            sourceComponent: addForm
        }
    }

    Flickable {
        id: scroller
        anchors.top: top.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        contentHeight: list.height
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickDeceleration: Native.flingDeceleration
        maximumFlickVelocity: Native.maxFlingVelocity

        Column {
            id: list
            x: root.leftInset
            width: scroller.width - root.leftInset - root.rightInset
            bottomPadding: 16 + root.bottomInset

            Loader {
                width: parent.width
                active: root.model.loading === true && root.shown === 0
                visible: active
                sourceComponent: Item {
                    height: 120
                    Spinner {
                        anchors.centerIn: parent
                        running: true
                    }
                }
            }

            Loader {
                width: parent.width
                active: root.model.loading !== true && root.shown === 0
                visible: active
                sourceComponent: emptyState
            }

            Repeater {
                model: root.shown > 0 ? root.branches : []
                delegate: Column {
                    id: branch
                    required property var modelData
                    width: list.width
                    topPadding: 8
                    Item {
                        width: parent.width
                        height: 32
                        Accessible.role: Accessible.Heading
                        Accessible.name: branch.modelData.label
                        Row {
                            x: 16
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 8
                            Text {
                                textFormat: Text.PlainText
                                text: branch.modelData.label
                                color: Theme.ink3
                                font: Theme.type.silk
                            }
                            Text {
                                textFormat: Text.PlainText
                                text: branch.modelData.rows.length
                                color: Theme.ink3
                                font: Theme.type.numSm
                            }
                        }
                    }
                    Repeater {
                        model: branch.modelData.rows
                        delegate: FriendItem {
                            required property var modelData
                            width: branch.width
                            person: modelData
                        }
                    }
                }
            }
        }
    }

    // Add friend: the username, Send request, then the outcome (the error
    // in danger ink, which typing clears; the success line in success ink).
    // A request that went out empties the field.
    Component {
        id: addForm
        Rectangle {
            id: form
            readonly property var status: root.model.add ?? { kind: "", message: "", sent: 0 }
            property int seenSent: 0
            width: parent ? parent.width : 0
            height: formColumn.implicitHeight + 1
            color: Theme.ply1
            Component.onCompleted: {
                seenSent = status.sent
                field.forceActiveFocus()
            }
            onStatusChanged: {
                if (status.sent !== seenSent) {
                    seenSent = status.sent
                    field.clear()
                }
            }
            Column {
                id: formColumn
                x: 16 + root.leftInset
                width: parent.width - 32 - root.leftInset - root.rightInset
                topPadding: 16
                bottomPadding: 12
                spacing: 8
                Text {
                    width: parent.width
                    wrapMode: Text.Wrap
                    text: qsTr("Add friends by their username.")
                    color: Theme.ink2
                    font: Theme.type.description
                }
                Row {
                    width: parent.width
                    spacing: 8
                    MobileField {
                        id: field
                        width: parent.width - send.width - parent.spacing
                        placeholderText: qsTr("Username")
                        invalid: form.status.kind === "error"
                        inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText
                        EnterKey.type: Qt.EnterKeySend
                        Accessible.name: qsTr("Username to add")
                        Accessible.description: outcome.text
                        onTextEdited: Friends.clearAdd()
                        onAccepted: if (text.trim().length > 0) Friends.sendRequest(text.trim())
                    }
                    KitButton {
                        id: send
                        size: "lg"
                        variant: "primary"
                        ground: Theme.ply1
                        text: qsTr("Send")
                        enabled: field.text.trim().length > 0
                        onClicked: {
                            Native.haptic(Native.Confirm)
                            Friends.sendRequest(field.text.trim())
                        }
                    }
                }
                Text {
                    id: outcome
                    width: parent.width
                    height: Math.max(16, implicitHeight)
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    text: form.status.message
                    color: form.status.kind === "error" ? Theme.dangerInk : Theme.successInk
                    font: Theme.type.caption
                    Accessible.role: Accessible.StaticText
                    Accessible.name: text
                }
            }
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 1
                color: Theme.line
            }
        }
    }

    Component {
        id: emptyState
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
                x: 32
                width: parent.width - 64
                visible: root.tab === "all" || root.tab === "online"
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                text: qsTr("Add a friend by their username to start a conversation.")
                color: Theme.ink2
                font: Theme.type.description
            }
            Item {
                visible: (root.tab === "all" || root.tab === "online") && !root.adding
                width: parent.width
                height: 16 + addKey.height
                KitButton {
                    id: addKey
                    y: 16
                    anchors.horizontalCenter: parent.horizontalCenter
                    variant: "secondary"
                    iconName: "invite"
                    text: qsTr("Add friend")
                    onClicked: root.adding = true
                }
            }
        }
    }

    // One person: the avatar with their presence (none when blocked), the
    // name over what they play, their status or the request's state, and
    // the keys for what can be done.
    component FriendItem: Column {
        id: item
        required property var person
        readonly property string kind: person.kind
        readonly property var act: root.actOf(person.userId)
        readonly property bool busy: act.busy.length > 0
        readonly property bool open: root.openId === person.userId
        readonly property string asking: root.confirming && root.confirming.userId === person.userId
            ? root.confirming.action : ""
        readonly property string game: person.game ?? ""
        readonly property string second: game.length > 0 ? ""
            : kind === "incoming" ? qsTr("Wants to be friends")
            : kind === "sent" ? qsTr("Request sent")
            : kind === "blocked" ? qsTr("Blocked")
            : person.statusLine.length > 0 ? person.statusLine
            : Theme.presenceWord(person.presence)

        width: parent ? parent.width : 0

        function activate() {
            Native.haptic(Native.Tick)
            if (kind === "friend")
                root.messageRequested(person.userId)
            else
                root.profileRequested(person.userId)
        }

        Rectangle {
            width: parent.width
            height: 64
            color: rowTap.pressed ? Theme.tint1 : item.open ? Theme.tint1 : "transparent"
            Behavior on color { ColorAnimation { duration: Theme.dur1 } }
            Accessible.role: Accessible.Button
            Accessible.name: item.kind === "friend" ? qsTr("Message %1").arg(item.person.name)
                : qsTr("%1, %2").arg(item.person.name).arg(item.second)
            Accessible.onPressAction: item.activate()

            TapHandler {
                id: rowTap
                onTapped: item.activate()
                onLongPressed: {
                    if (item.kind !== "friend")
                        return
                    Native.haptic(Native.LongPress)
                    root.confirming = null
                    root.openId = item.person.userId
                }
            }

            PresenceAvatar {
                id: face
                x: 16
                anchors.verticalCenter: parent.verticalCenter
                size: 40
                initials: item.person.initials
                source: item.person.avatar
                presence: item.kind === "blocked" ? "" : item.person.presence
                ground: Theme.ply0
                // A MouseArea, so the press stops here instead of opening
                // the DM (the desktop's FriendRow).
                MouseArea {
                    anchors.fill: parent
                    anchors.margins: -4
                    onClicked: {
                        Native.haptic(Native.Tick)
                        root.profileRequested(item.person.userId)
                    }
                }
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("View %1's profile").arg(item.person.name)
                Accessible.ignored: false
            }

            Column {
                anchors.left: face.right
                anchors.leftMargin: 12
                anchors.right: keys.left
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                DisplayName {
                    width: parent.width
                    text: item.person.name
                    nameStyle: item.person.nameStyle ?? ""
                    color: Theme.ink1
                    font: Theme.type.rowUnread
                    ground: Theme.ply0
                }
                Loader {
                    width: parent.width
                    active: item.game.length > 0
                    visible: active
                    sourceComponent: GameLine {
                        game: item.game
                        icon: item.person.gameIcon ?? ""
                    }
                }
                Text {
                    width: parent.width
                    visible: item.second.length > 0
                    textFormat: Text.PlainText
                    text: item.second
                    elide: Text.ElideRight
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }

            Row {
                id: keys
                anchors.right: parent.right
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                spacing: 8
                KitButton {
                    visible: item.kind === "incoming"
                    anchors.verticalCenter: parent.verticalCenter
                    variant: "primary"
                    text: qsTr("Accept")
                    loading: item.act.busy === "accept"
                    enabled: !item.busy
                    Accessible.name: qsTr("Accept %1").arg(item.person.name)
                    onClicked: {
                        Native.haptic(Native.Confirm)
                        root.act(item.person, "accept")
                    }
                }
                IconKey {
                    visible: item.kind === "incoming"
                    iconName: "close"
                    tip: qsTr("Ignore %1").arg(item.person.name)
                    enabled: !item.busy
                    onClicked: root.ask(item.person, "decline")
                }
                KitButton {
                    visible: item.kind === "sent"
                    anchors.verticalCenter: parent.verticalCenter
                    variant: "secondary"
                    text: qsTr("Cancel")
                    enabled: !item.busy
                    Accessible.name: qsTr("Cancel the request to %1").arg(item.person.name)
                    onClicked: root.ask(item.person, "cancel")
                }
                KitButton {
                    visible: item.kind === "blocked"
                    anchors.verticalCenter: parent.verticalCenter
                    variant: "secondary"
                    text: qsTr("Unblock")
                    loading: item.act.busy === "unblock"
                    enabled: !item.busy
                    Accessible.name: qsTr("Unblock %1").arg(item.person.name)
                    onClicked: root.act(item.person, "unblock")
                }
                IconKey {
                    visible: item.kind === "friend"
                    iconName: "chat"
                    tip: qsTr("Message %1").arg(item.person.name)
                    onClicked: {
                        Native.haptic(Native.Tick)
                        root.messageRequested(item.person.userId)
                    }
                }
                IconKey {
                    visible: item.kind === "friend"
                    iconName: "more"
                    tone: item.open ? "toggled" : "ghost"
                    tip: qsTr("More for %1").arg(item.person.name)
                    onClicked: {
                        root.confirming = null
                        root.openId = item.open ? "" : item.person.userId
                    }
                }
            }
        }

        // A friend's other actions, in the row.
        Loader {
            width: parent.width
            active: item.open && item.asking.length === 0
            visible: active
            sourceComponent: Rectangle {
                height: actionRow.height + 12
                color: Theme.tint1
                Row {
                    id: actionRow
                    x: 16
                    width: parent.width - 32
                    spacing: 8
                    KitButton {
                        width: (parent.width - 2 * parent.spacing) / 3
                        variant: "secondary"
                        iconName: "profile"
                        text: qsTr("Profile")
                        onClicked: root.profileRequested(item.person.userId)
                    }
                    KitButton {
                        width: (parent.width - 2 * parent.spacing) / 3
                        variant: "destructive"
                        iconName: "userRemove"
                        text: qsTr("Remove")
                        Accessible.name: qsTr("Remove %1 from friends").arg(item.person.name)
                        onClicked: root.ask(item.person, "remove")
                    }
                    KitButton {
                        width: (parent.width - 2 * parent.spacing) / 3
                        variant: "destructive"
                        iconName: "ban"
                        text: qsTr("Block")
                        Accessible.name: qsTr("Block %1").arg(item.person.name)
                        onClicked: root.ask(item.person, "block")
                    }
                }
            }
        }

        // The step being confirmed.
        Loader {
            width: parent.width
            active: item.asking.length > 0
            visible: active
            sourceComponent: Item {
                height: strip.height + 12
                InlineConfirm {
                    id: strip
                    x: 16
                    width: parent.width - 32
                    consequence: item.asking === "remove"
                        ? qsTr("%1 will be removed from your friends. You can send a new request later.").arg(item.person.name)
                        : item.asking === "block"
                        ? qsTr("%1 will be removed from your friends and can't message you or send you requests. You can unblock them later.").arg(item.person.name)
                        : item.asking === "decline" ? qsTr("Ignore the friend request from %1?").arg(item.person.name)
                        : qsTr("Cancel your friend request to %1?").arg(item.person.name)
                    confirmLabel: item.asking === "remove" ? qsTr("Remove")
                        : item.asking === "block" ? qsTr("Block")
                        : item.asking === "decline" ? qsTr("Ignore") : qsTr("Cancel request")
                    dismissLabel: qsTr("Keep")
                    busy: item.busy
                    error: item.act.failed === item.asking ? item.act.error : ""
                    onCancelled: root.confirming = null
                    onConfirmed: root.act(item.person, item.asking)
                }
            }
        }

        // A failed Accept or Unblock (the rest show in their strip).
        Text {
            x: 68
            width: parent.width - 84
            visible: item.asking.length === 0 && item.act.error.length > 0
                && (item.act.failed === "accept" || item.act.failed === "unblock")
            bottomPadding: 8
            textFormat: Text.PlainText
            text: item.act.error
            wrapMode: Text.Wrap
            color: Theme.dangerInk
            font: Theme.type.caption
        }
    }

    component FriendsTab: Item {
        id: tabKey
        property string value
        property string text
        property int count: 0
        readonly property bool on: root.tab === value
        width: parent ? parent.width / 4 : 0
        height: parent ? parent.height : 48
        Accessible.role: Accessible.PageTab
        Accessible.name: count > 0 ? qsTr("%1, %n incoming request(s)", "", count).arg(text) : text
        Accessible.selected: on
        Accessible.onPressAction: root.tab = value
        Row {
            anchors.centerIn: parent
            spacing: 6
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: tabKey.text
                color: tabKey.on ? Theme.ink1 : Theme.ink3
                Behavior on color { ColorAnimation { duration: Theme.dur1 } }
                font: Theme.type.control
            }
            CountBadge {
                anchors.verticalCenter: parent.verticalCenter
                count: tabKey.count
            }
        }
        TapHandler {
            onTapped: {
                if (tabKey.on)
                    return
                Native.haptic(Native.Tick)
                root.tab = tabKey.value
            }
        }
    }
}
