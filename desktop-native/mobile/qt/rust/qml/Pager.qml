pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Home's three pages (apps/mobile HomePager.tsx): the drawer (rail, tree or
// DMs, your account) underneath, the open conversation over it, and a
// channel's members to its right. From the chat, a swipe right or the
// header's menu key slides the chat aside to a 56dp peek; a swipe left, a
// tap on the peek or picking a conversation slides it back. The header's
// members key slides the chat out to the left with the members page
// following it in; a swipe right or Back returns. There is no swipe left
// into members: a leftward drag on the chat belongs to a message, and
// Android's gesture navigation claims the right edge for Back. The chat
// stays mounted throughout. With no conversation open the drawer fills the
// screen and the rest is out of reach.
//
// From 840dp the drawer (324dp) and the chat sit side by side and nothing
// pans; members slide over the chat's right edge, and from 1200dp they are
// a column of their own.
Item {
    id: pager

    // "drawer" | "chat" | "members". Set through show().
    property string page: "drawer"

    signal profileRequested(string userId)
    signal settingsRequested()
    signal friendsRequested()
    signal voiceRequested(string channelId)
    signal callRequested()

    readonly property bool wide: width >= 840
    readonly property bool docked: width >= 1200
    readonly property bool hasChat: Nav.activeChannelId.length > 0
    // A server channel has members; a DM's other half is its header.
    readonly property bool hasMembers: hasChat && !Nav.home

    // How much of the chat stays in view beside the open drawer.
    readonly property int peek: 56
    readonly property real drawerWidth: wide ? 324 : hasChat ? width - peek : width
    readonly property real membersWidth: docked ? 280 : wide ? 320 : width
    // The members column from 1200dp: shown until its key hides it.
    property bool dockedMembers: true
    readonly property bool membersDocked: docked && dockedMembers && hasMembers

    // The conversation is on screen (it counts as read only then).
    readonly property bool chatShown: hasChat && (wide || page === "chat")
    readonly property bool membersShown: hasMembers && (membersDocked || page === "members")
    // Something Back steps away from before it leaves the app: what lies
    // over the chat (its image viewer), then a page.
    readonly property bool canGoBack: chat.overlayShown || canStepBack
    readonly property bool canStepBack: wide ? page === "members" && !docked : page !== "drawer"

    // The chat's offset from its resting place: on a phone 0 covers the
    // drawer, drawerWidth peeks at it and -width has slid out for members;
    // side by side only the members panel moves (-membersWidth is open).
    property real offset: restOf(page)
    property real target: offset

    // A drag this share of the way across changes page without a flick; a
    // flick faster than `fling` (dp/s) decides on its own.
    readonly property real commit: 0.35
    readonly property real fling: 500
    // The drawer drifts this far under the chat as it closes.
    readonly property real parallax: 24

    function restOf(which) {
        if (wide)
            return which === "members" && !docked ? -membersWidth : 0
        if (which === "chat")
            return 0
        return which === "members" ? -width : drawerWidth
    }

    // Slide to a page, carrying `velocity` (dp/s) from the finger.
    function show(next, velocity) {
        // A conversation picked while none was open is still on its way
        // (the core opens it): the chat slides in when it is there.
        wantChat = next === "chat" && !hasChat
        if (wantChat)
            wantTimer.restart()
        if (next !== "drawer" && !hasChat)
            next = "drawer"
        if (next === "members" && !hasMembers)
            next = "chat"
        if (wide && next === "drawer" && hasChat)
            next = "chat"
        if (next === "members" && docked) {
            dockedMembers = true
            next = "chat"
        }
        if (next !== "chat")
            Qt.inputMethod.hide()
        page = next
        const rest = restOf(next)
        target = rest
        if (Theme.reducedMotion || offset === rest) {
            spring.halt()
            offset = rest
            return
        }
        spring.kick(velocity ?? 0)
    }

    // The header's members key: the page on a phone, the panel beside it.
    function toggleMembers() {
        if (docked)
            dockedMembers = !dockedMembers
        else
            show(page === "members" ? "chat" : "members")
    }

    property bool wantChat: false
    Timer {
        id: wantTimer
        interval: 8000
        onTriggered: pager.wantChat = false
    }

    // Opening a DM that may still be on its way (made on first contact):
    // the chat slides in once theirs is the open conversation.
    property string awaitedDm: ""
    function showDmWhenOpen(userId) {
        if (Nav.dmUserId === userId && hasChat) {
            show("chat")
            return
        }
        awaitedDm = userId
        awaitTimer.restart()
    }
    Timer {
        id: awaitTimer
        interval: 8000
        onTriggered: pager.awaitedDm = ""
    }
    Connections {
        target: Nav
        function onDmUserIdChanged() {
            if (pager.awaitedDm.length === 0 || Nav.dmUserId !== pager.awaitedDm)
                return
            pager.awaitedDm = ""
            // After the conversation itself changed (the same update).
            Qt.callLater(pager.show, "chat")
        }
    }

    // Back steps a page left: members to the chat, the chat to the drawer.
    function previous() {
        return page === "members" ? "chat" : "drawer"
    }
    function back() {
        if (chat.closeOverlay())
            return true
        if (!canStepBack)
            return false
        show(previous())
        return true
    }
    // The predictive back gesture: the page follows the finger part of the
    // way, then the spring finishes or returns it.
    function backPreview(progress) {
        if (chat.overlayShown) {
            chat.backProgress = progress
            return
        }
        // The shim ends a committed gesture with a progress of 0 before
        // it invokes Back: the spring starts from where the finger left.
        if (!canStepBack || Theme.reducedMotion || progress <= 0)
            return
        spring.halt()
        const rest = restOf(page)
        offset = rest + (restOf(previous()) - rest) * progress * 0.4
    }
    function backCancel() {
        chat.backProgress = 0
        show(page)
    }

    // Pages that stopped existing (the conversation closed, a DM has no
    // members) and layout changes (rotation, a resized window) land
    // without travel.
    function snap() {
        let next = page
        if (!hasChat)
            next = "drawer"
        else if (next === "members" && (!hasMembers || docked))
            next = "chat"
        else if (wide && next === "drawer")
            next = "chat"
        page = next
        spring.halt()
        target = restOf(next)
        offset = target
    }
    onWideChanged: snap()
    onDockedChanged: snap()
    onWidthChanged: snap()
    onDrawerWidthChanged: snap()
    onMembersWidthChanged: snap()
    onHasMembersChanged: snap()
    onHasChatChanged: {
        snap()
        if (hasChat && wantChat)
            Qt.callLater(pager.show, "chat")
    }

    // DESIGN's panel spring: critically damped (no overshoot), settling in
    // about 250ms, started at the finger's velocity. Stiffness 360 at unit
    // mass, as the Expo pager's; damping 2*sqrt(360).
    FrameAnimation {
        id: spring

        property real velocity: 0
        property real side: 0
        readonly property real stiffness: 360
        readonly property real damping: 37.95

        function kick(v) {
            velocity = v
            side = Math.sign(pager.offset - pager.target)
            running = true
        }
        function halt() {
            running = false
            velocity = 0
        }
        onTriggered: {
            // Sub-steps keep a dropped frame from overshooting the rest.
            const steps = 4
            const h = Math.min(frameTime, 0.034) / steps
            let x = pager.offset
            let v = velocity
            for (let i = 0; i < steps; ++i) {
                v += (-stiffness * (x - pager.target) - damping * v) * h
                x += v * h
            }
            const arrived = Math.sign(x - pager.target) !== side
                || (Math.abs(x - pager.target) < 0.5 && Math.abs(v) < 20)
            if (arrived) {
                halt()
                pager.offset = pager.target
                return
            }
            velocity = v
            pager.offset = x
        }
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.ply0
    }

    DrawerPage {
        id: drawer
        chatHasRows: chat.hasRows
        x: pager.wide ? 0
            : -pager.parallax * (1 - Math.max(0, Math.min(1, pager.offset / Math.max(1, pager.drawerWidth))))
        width: pager.drawerWidth
        height: parent.height
        enabled: pager.wide || pager.page === "drawer"
        onConversationOpened: pager.show("chat")
        onProfileRequested: userId => pager.profileRequested(userId)
        onSettingsRequested: pager.settingsRequested()
        onFriendsRequested: pager.friendsRequested()
        onVoiceRequested: channelId => pager.voiceRequested(channelId)
        onCallRequested: pager.callRequested()
    }

    Item {
        id: chatSlot
        x: pager.wide ? pager.drawerWidth : pager.offset
        width: pager.wide ? pager.width - pager.drawerWidth - (pager.membersDocked ? pager.membersWidth : 0) : pager.width
        height: parent.height
        visible: pager.hasChat || pager.wide

        // The chat's edge over the drawer: a soft shadow and a hairline.
        Rectangle {
            visible: !pager.wide
            anchors.right: parent.left
            width: 12
            height: parent.height
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0; color: "transparent" }
                GradientStop { position: 1; color: Theme.shadow1 }
            }
        }

        ChatPage {
            id: chat
            anchors.fill: parent
            visible: pager.hasChat
            enabled: pager.chatShown
            menuKey: !pager.wide
            membersKey: pager.hasMembers
            membersOn: pager.membersShown
            onMenuRequested: pager.show("drawer")
            onMembersRequested: pager.toggleMembers()
            onProfileRequested: userId => pager.profileRequested(userId)
        }

        // Side by side with nothing open: where the conversation will be.
        Column {
            anchors.centerIn: parent
            width: Math.min(parent.width - 48, 320)
            spacing: 4
            visible: pager.wide && !pager.hasChat
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: qsTr("No conversation open")
                color: Theme.ink1
                font: Theme.type.panelTitle
            }
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                text: qsTr("Pick a channel or a direct message on the left.")
                color: Theme.ink3
                font: Theme.type.description
            }
        }

        Rectangle {
            width: 1
            height: parent.height
            color: Theme.line
        }

        // The peek: dimmed as the chat slides aside; a tap brings it back.
        Rectangle {
            anchors.fill: parent
            visible: !pager.wide && opacity > 0
            color: Theme.scrim
            opacity: Math.max(0, Math.min(1, pager.offset / Math.max(1, pager.drawerWidth)))
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Back to the conversation")
            Accessible.onPressAction: pager.show("chat")
            TapHandler {
                enabled: pager.page === "drawer"
                onTapped: pager.show("chat")
            }
        }
    }

    // Side by side under 1200dp the members panel covers the chat's right
    // edge: a tap beside it closes it.
    Rectangle {
        x: pager.drawerWidth
        width: pager.width - x
        height: parent.height
        visible: pager.wide && !pager.docked && opacity > 0
        color: Theme.scrim
        opacity: Math.max(0, Math.min(1, -pager.offset / pager.membersWidth))
        TapHandler {
            enabled: pager.page === "members"
            // The tap is the scrim's alone, not also the chat's under it.
            gesturePolicy: TapHandler.ReleaseWithinBounds
            onTapped: pager.show("chat")
        }
    }

    // Built on first open, then kept, so going back and forth is instant.
    Loader {
        id: membersSlot
        property bool built: false
        x: pager.docked ? pager.width - pager.membersWidth : pager.width + pager.offset
        width: pager.membersWidth
        height: parent.height
        visible: pager.membersDocked || (!pager.docked && pager.offset < 0)
        enabled: pager.membersShown
        active: built
        sourceComponent: MembersPage {
            closeKey: !pager.docked
            onCloseRequested: pager.show("chat")
            onProfileRequested: userId => pager.profileRequested(userId)
        }
        Binding on built {
            when: pager.membersShown
            value: true
            restoreMode: Binding.RestoreNone
        }
    }

    // The pan. An item of its own over the pages: a handler here sees every
    // touch without taking it, and claims it only once it has travelled
    // sideways; a list that started scrolling first keeps its touch.
    Item {
        anchors.fill: parent

        // Which way the touch set off, read before the pan's threshold: from
        // the chat or members only a rightward drag pans, from the drawer
        // only a leftward one. Anything else (a leftward drag on a message,
        // a scroll) switches the pan off until the finger lifts, so it never
        // claims that touch.
        PointHandler {
            id: probe

            property bool decided: false

            target: null
            onActiveChanged: {
                decided = false
                if (!active)
                    pan.wrongWay = false
            }
            onPointChanged: {
                if (!active || decided || pan.active)
                    return
                const dx = point.position.x - point.pressPosition.x
                const dy = point.position.y - point.pressPosition.y
                if (Math.abs(dx) < 6 && Math.abs(dy) < 6)
                    return
                decided = true
                const rightward = pager.page !== "drawer"
                pan.wrongWay = Math.abs(dy) > Math.abs(dx) || (rightward ? dx < 0 : dx > 0)
            }
        }

        DragHandler {
            id: pan

            property bool wrongWay: false

            property real from: 0
            property real startX: 0
            property real lastX: 0
            property real lastTime: 0
            property real speed: 0
            property int samples: 0

            target: null
            enabled: !pager.wide && pager.hasChat && !wrongWay && !chat.overlayShown
            xAxis.enabled: true
            yAxis.enabled: false
            dragThreshold: 14

            onActiveChanged: {
                if (active) {
                    spring.halt()
                    from = pager.offset
                    // From where the finger went down, so the page is
                    // under it once the pan claims the touch.
                    startX = centroid.scenePressPosition.x
                    lastX = centroid.scenePosition.x
                    lastTime = Date.now()
                    speed = centroid.velocity.x
                    samples = 0
                    Qt.inputMethod.hide()
                    follow()
                    return
                }
                // A finger that stopped before lifting is not a flick.
                if (Date.now() - lastTime > 100)
                    speed = 0
                const fromMembers = from < -pager.width / 2
                const moved = Math.abs(pager.offset - from)
                let next
                if (fromMembers) {
                    const back = speed > pager.fling || (speed > -pager.fling && moved > pager.width * pager.commit)
                    next = back ? "chat" : "members"
                } else {
                    const fromChat = from < pager.drawerWidth / 2
                    const travelled = moved > pager.drawerWidth * pager.commit
                    const toDrawer = speed > pager.fling
                        || (speed > -pager.fling && (fromChat ? travelled : !travelled))
                    next = toDrawer ? "drawer" : "chat"
                }
                if (next !== pager.page)
                    Native.haptic(Native.Tick)
                pager.show(next, speed)
            }
            onCentroidChanged: if (active) follow()

            function follow() {
                const x = centroid.scenePosition.x
                const now = Date.now()
                if (now > lastTime && x !== lastX) {
                    // The finger's speed, smoothed over the last few moves
                    // (the handler's own estimate until there are two).
                    const v = (x - lastX) * 1000 / (now - lastTime)
                    speed = samples === 0 ? v : 0.5 * speed + 0.5 * v
                    samples += 1
                    lastX = x
                    lastTime = now
                }
                // Members slides between itself and the chat, the drawer
                // likewise.
                const fromMembers = from < -pager.width / 2
                const low = fromMembers ? -pager.width : 0
                const high = fromMembers ? 0 : pager.drawerWidth
                pager.offset = Math.min(high, Math.max(low, from + x - startX))
            }
        }
    }
}
