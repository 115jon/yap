import QtQuick
import QtQuick.Controls.Basic
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The mobile shell: one full-screen window drawn edge to edge (the system
// bars float over it; SafeArea keeps content clear of them), the desktop's
// sign-in phases, and the navigation: Home's pager (drawer, chat, members)
// with pages and sheets stacked over it (a profile, Settings, Friends, the
// call), a DM call's incoming sheet over everything, and the platform's
// back gesture stepping through them. The pages are per platform; the rows
// inside them are the desktop's.
Window {
    id: window

    visible: true
    // Edge to edge: the window draws under the status and navigation bars.
    flags: Qt.Window | Qt.ExpandedClientAreaHint
    color: Theme.ply0
    title: "Yap"

    // The theme picked in Settings (kept on this device, or synced from the
    // account); "system", the phone's default, follows the light or dark
    // mode, live.
    Binding {
        target: Theme
        property: "themeId"
        value: Settings.theme.length > 0 && Settings.theme !== "system" ? Settings.theme
            : Application.styleHints.colorScheme === Qt.ColorScheme.Light ? "light" : "dark"
    }
    Binding {
        target: Theme
        property: "reducedMotion"
        // Also while the app is off screen (suspended, its surface gone):
        // Qt keeps running in the background (the manifest's
        // background_running, for a call's view updates), and a talk LED's
        // fade there would only tick the animation timer for a frame nobody
        // draws.
        value: Native.reducedMotion || Qt.application.state === Qt.ApplicationSuspended
    }
    // A phone: fingers, no hover, and the system's text size times this
    // phone's factor (Settings > Text size), within what the layouts take.
    Binding { target: Theme; property: "touch"; value: true }
    Binding { target: Theme; property: "hoverUi"; value: false }
    Binding {
        target: Theme
        property: "textScale"
        value: Math.max(0.85, Math.min(2.0, Native.fontScale * Prefs.textFactor))
    }
    // Dark status and navigation bar icons on the light themes (a picked
    // theme can differ from the system's mode).
    Binding {
        target: Native
        property: "lightBars"
        value: Theme.lightGround
    }

    Component.onCompleted: {
        // The core starts out focused; say so before anything loads, or a
        // conversation restored under the drawer counts as read.
        Chat.setFocused(reading)
        Session.start()
    }

    readonly property bool ready: Session.phase === Session.Ready

    // Foreground / background: the core keeps the gateway as for a shown
    // window, and the snapshot cache is committed on the way out.
    Connections {
        target: Qt.application
        function onStateChanged() {
            const active = Qt.application.state === Qt.ApplicationActive
            Session.setVisible(active)
            if (!active)
                Native.flushCache()
        }
    }

    // The open conversation counts as read (and its mentions as seen) only
    // while it is on screen with nothing over it and the app in front, as
    // the desktop ties it to window focus.
    readonly property bool reading: ready && stack.depth === 1 && pager.chatShown
        && Qt.application.state === Qt.ApplicationActive
    onReadingChanged: Chat.setFocused(reading)

    // Push notifications: the conversation being read gets none (and its
    // notification goes), and a tapped one opens once signed in.
    Binding {
        target: Native
        property: "activeConversation"
        value: window.reading ? Nav.activeChannelId : ""
    }
    function openFromNotification() {
        if (!ready)
            return
        const request = Native.takeOpenRequest()
        if (!request.channelId)
            return
        closeTop(true)
        // The core opens a DM the list does not have yet once it answers;
        // the chat slides in when the conversation is the open one.
        window.openUnseen(() => Nav.openChannel(request.channelId, request.serverId))
        if (request.userId)
            pager.showDmWhenOpen(request.userId)
        else
            pager.showChannelWhenOpen(request.channelId)
    }
    Connections {
        target: Native
        function onOpenRequested() {
            window.openFromNotification()
        }
    }

    // Links the app was opened with (an invite or a chat link on the web
    // host, or yap://), once signed in: the core opens what they point at.
    // An invite raises its landing (Invites.landingRequested); a channel
    // link slides the chat in once that channel is the open one.
    function openFromLink() {
        if (!ready)
            return
        const url = Native.takeLinkRequest()
        if (url.length === 0)
            return
        const channelId = Invites.linkChannel(url)
        if (!Invites.openLink(url)) {
            console.warn("[link] not a link to Yap")
            return
        }
        if (channelId.length > 0) {
            closeTop(true)
            pager.showChannelWhenOpen(channelId)
        }
    }
    Connections {
        target: Native
        function onLinkRequested() {
            window.openFromLink()
        }
    }
    // An invite's landing, from a link here or one tapped in a message.
    Connections {
        target: Invites
        function onLandingRequested() {
            window.openLanding()
        }
    }
    function openLanding() {
        if (!ready || stack.currentItem?.isLanding === true)
            return
        closeTop(true)
        Qt.inputMethod.hide()
        stack.sheet = false
        stack.push(landingPage, {}, StackView.PushTransition)
    }

    // A conversation opened by the app (a notification, a profile's
    // Message): the core hears it was picked out of view, as a pick in the
    // drawer is, so whether it shows its unread divider does not depend on
    // which arrives first, the page or the chat sliding in (focus).
    function openUnseen(open) {
        Chat.setFocused(false)
        open()
        Qt.callLater(() => Chat.setFocused(window.reading))
    }

    // A profile, over whatever is showing.
    function openProfile(userId) {
        // One sheet per person: a second tap must not stack another.
        if (userId.length === 0 || stack.busy || stack.currentItem?.userId === userId)
            return
        Qt.inputMethod.hide()
        stack.sheet = true
        stack.push(profilePage, { userId: userId })
    }
    function openSettings() {
        Qt.inputMethod.hide()
        stack.sheet = false
        stack.push(settingsPage)
    }
    // Edit profile, over Settings or your own profile.
    function openProfileEditor() {
        if (stack.busy || stack.currentItem?.isProfileEditor === true)
            return
        Qt.inputMethod.hide()
        stack.sheet = false
        stack.push(profileEditPage)
    }
    // A message's thread, over the chat.
    function openThread(messageId) {
        if (messageId.length === 0 || stack.busy || stack.currentItem?.rootId === messageId)
            return
        Qt.inputMethod.hide()
        stack.sheet = false
        stack.push(threadPage, { rootId: messageId })
    }
    function openFriends() {
        if (stack.busy)
            return
        Qt.inputMethod.hide()
        stack.sheet = false
        stack.push(friendsPage)
    }
    // A voice channel's row: the call page if we are in it, else a join,
    // once the microphone is allowed (the system asks the first time).
    property string wantedVoice: ""
    function joinVoice(channelId) {
        if (channelId.length === 0)
            return
        if (Voice.channelId === channelId) {
            openCall()
            return
        }
        afterMicrophone = null
        refusedMicrophone = null
        wantedVoice = channelId
        Native.requestMicrophone()
    }
    function openCall() {
        if (stack.busy || stack.currentItem?.isCall === true)
            return
        Qt.inputMethod.hide()
        stack.sheet = false
        stack.push(callPage)
    }
    // The call page once the stack is still: an answer closes what is on
    // top first (a profile, a thread, Settings), and that pop animates.
    property bool callWanted: false
    function openCallSoon() {
        if (stack.busy) {
            callWanted = true
            return
        }
        callWanted = false
        openCall()
    }
    Connections {
        target: stack
        function onBusyChanged() {
            if (stack.busy || !window.callWanted)
                return
            window.callWanted = false
            if (window.ready)
                window.openCall()
        }
    }
    // What a DM call does once the microphone is allowed (joinVoice's
    // channel goes the same way).
    property var afterMicrophone: null
    // What a refusal does besides the microphone sheet (an answer declines).
    property var refusedMicrophone: null
    function withMicrophone(then, refused) {
        wantedVoice = ""
        afterMicrophone = then
        refusedMicrophone = refused ?? null
        Native.requestMicrophone()
    }
    Connections {
        target: Native
        function onMicrophoneAnswered(granted) {
            const channelId = window.wantedVoice
            const then = window.afterMicrophone
            const refused = window.refusedMicrophone
            window.wantedVoice = ""
            window.afterMicrophone = null
            window.refusedMicrophone = null
            // A DM call's step waits for sign-in itself (an answer from the
            // lock screen can come while the session still loads).
            if (!then && (channelId.length === 0 || !window.ready))
                return
            if (!granted) {
                if (refused)
                    refused()
                microphoneSheet.open()
                return
            }
            Native.haptic(Native.Confirm)
            if (then) {
                then()
                return
            }
            Voice.join(channelId)
            window.openCall()
        }
    }

    // DM calls. The open DM's call and the one ringing us, as Yap.Ui's
    // model reads the core's (the desktop's call area reads the same).
    DmCallModel { id: dmCall }

    // The header's call key: join the call going on, else ask, then ring.
    function startDmCall() {
        const channelId = Nav.activeChannelId
        if (channelId.length === 0)
            return
        if (dmCall.existing) {
            withMicrophone(() => {
                Calls.join(channelId, false)
                window.openCall()
            })
            return
        }
        callConfirm.channelId = channelId
        callConfirm.name = Nav.activeChannelName
        callConfirm.leaves = Voice.channelId.length > 0 && !dmCall.inDmCall ? Voice.channelName : ""
        callConfirm.open()
    }
    function dmCallAction(action) {
        const channelId = Nav.activeChannelId
        if (action === "start")
            startDmCall()
        else if (action === "join")
            withMicrophone(() => {
                Calls.join(channelId, false)
                window.openCall()
            })
        else if (action === "answer")
            answerRing()
        else if (action === "decline")
            declineRing()
        else if (action === "open")
            openCall()
    }

    // The ring the incoming sheet shows: the core's (CALL_RING over the
    // gateway), else the shim's (a push the core never saw: the app started
    // from it, or the gateway is closed in the background).
    property string handledRing: ""
    readonly property var ringing: {
        if (dmCall.incoming && dmCall.callId.length > 0) {
            return {
                "callId": dmCall.callId,
                "channelId": dmCall.callChannelId,
                "callerId": dmCall.remote.id ?? "",
                "callerName": dmCall.remote.name ?? "",
                "callerAvatar": dmCall.remote.avatar ?? "",
                "core": true
            }
        }
        const shim = Native.incomingCall
        if ((shim.callId ?? "").length > 0)
            return Object.assign({ "core": false }, shim)
        return {}
    }
    readonly property var shownRing: (ringing.callId ?? "").length > 0 && ringing.callId !== handledRing ? ringing : ({})

    // The core's ring goes to the shim, which rings once per call id
    // whichever came first (the ringtone in front, else the notification),
    // and its end stops it there (a missed one leaves a notification).
    property string coreRingId: ""
    readonly property string coreRing: dmCall.incoming ? dmCall.callId : ""
    onCoreRingChanged: {
        if (coreRingId.length > 0 && coreRingId !== coreRing)
            Native.stopIncoming(coreRingId, dmCall.status === "active" ? "accepted"
                : dmCall.endReason.length > 0 ? dmCall.endReason : "ended")
        coreRingId = coreRing
        if (coreRing.length > 0)
            Native.ringIncoming(ringing)
    }

    // Answer in the sheet (or the strip): once the microphone is allowed.
    // The core's ring is answered over the gateway (it opens the DM and
    // joins when the server says accepted); the shim's over REST, then the
    // DM's call is joined here.
    function answerRing() {
        const ring = shownRing
        if ((ring.callId ?? "").length === 0)
            return
        withMicrophone(() => {
            // Stopped ringing while the system asked (cancelled, timed out):
            // nothing to answer, and a join would sit alone in its room.
            if ((window.ringing.callId ?? "") !== ring.callId)
                return
            window.handledRing = ring.callId
            if (ring.core && dmCall.incoming && dmCall.callId === ring.callId) {
                Native.answerIncoming(ring.callId, false)
                dmCall.accept(false)
                window.closeTop(true)
                window.openCallSoon()
            } else {
                // Over REST: the shim hands the call back (callAnswered)
                // only once the server took the answer, and says so if not.
                Native.answerIncoming(ring.callId, true)
            }
        }, () => {
            // No microphone, no call: the ring is declined, and the
            // microphone sheet shows (not under the incoming sheet).
            if ((window.ringing.callId ?? "") === ring.callId)
                window.declineRing()
        })
    }
    function declineRing() {
        const ring = shownRing
        if ((ring.callId ?? "").length === 0)
            return
        handledRing = ring.callId
        const core = ring.core && dmCall.incoming && dmCall.callId === ring.callId
        Native.declineIncoming(ring.callId, !core)
        if (core)
            dmCall.decline()
    }

    // A call answered over REST (its notification, the lock screen, or the
    // sheet for a ring the core never saw), which the server took: open its
    // DM and join its call once the DM is the open one and the core sees
    // the call there (a cold start's list may still load). The join waits
    // a minute at most (IncomingCalls.ANSWERED_JOIN_MS), and not past
    // another conversation opening or a sign-out: the microphone goes live
    // only for the call just answered.
    property string answeredChannel: ""
    property string answeredCaller: ""
    property string answeredCallId: ""
    // The conversation open when its DM was asked for (the core's answer
    // comes later): any other one opening before the DM is the user's.
    property string answeredFrom: ""
    property bool answeredOpening: false
    // Its DM was the open one: leaving it is the user's too.
    property bool answeredSeen: false
    Timer {
        id: answeredTimer
        interval: 60000
        onTriggered: window.dropAnswered()
    }
    function dropAnswered() {
        answeredTimer.stop()
        answeredChannel = ""
        answeredCaller = ""
        answeredCallId = ""
        answeredFrom = ""
        answeredOpening = false
        answeredSeen = false
    }
    function joinAnswered(call) {
        dropAnswered()
        if ((call.channelId ?? "").length === 0)
            return
        answeredChannel = call.channelId
        answeredCaller = call.callerId ?? ""
        answeredCallId = call.callId ?? ""
        // What is left of the shim's minute (a cold start took some).
        answeredTimer.interval = Math.max(1, Math.min(60000, call.leftMs ?? 60000))
        answeredTimer.restart()
        if (ready)
            openAnswered()
    }
    function openAnswered() {
        if (answeredChannel.length === 0)
            return
        closeTop(true)
        const channelId = answeredChannel
        answeredFrom = Nav.activeChannelId
        answeredOpening = true
        openUnseen(() => Nav.openChannel(channelId, ""))
        if (answeredCaller.length > 0)
            pager.showDmWhenOpen(answeredCaller)
        Qt.callLater(joinWhenOpen)
    }
    // The answered call still rings or goes on in its DM, as the core sees
    // it: that ring itself, or the DM's call area (someone in its call).
    // Another ring in the DM is a new call: joining it would answer it.
    function answeredLive(channelId, callId) {
        if (dmCall.callChannelId === channelId && dmCall.callId.length > 0 && dmCall.status !== "idle")
            return dmCall.callId === callId && (dmCall.status === "ringing_incoming" || dmCall.status === "active")
        return Nav.activeChannelId === channelId && dmCall.shown
    }
    function joinWhenOpen() {
        if (!ready || answeredChannel.length === 0)
            return
        const open = Nav.activeChannelId
        if (open !== answeredChannel) {
            if (answeredOpening && open.length > 0 && (answeredSeen || open !== answeredFrom))
                dropAnswered()
            return
        }
        answeredSeen = true
        // Not (yet) in the core's view: the view's next change asks again,
        // until the timer gives up.
        if (!answeredLive(answeredChannel, answeredCallId))
            return
        const channelId = answeredChannel
        const callId = answeredCallId
        dropAnswered()
        withMicrophone(() => {
            // Still there after the system's question.
            if (Nav.activeChannelId !== channelId || !window.answeredLive(channelId, callId))
                return
            Calls.join(channelId, false)
            window.openCallSoon()
        })
    }
    Connections {
        target: Nav
        function onActiveChannelIdChanged() { window.joinWhenOpen() }
    }
    Connections {
        target: dmCall
        function onShownChanged() { window.joinWhenOpen() }
        function onStatusChanged() { window.joinWhenOpen() }
    }
    function takeAnswered() {
        const call = Native.takeAnsweredCall()
        // Signed out: the answer went with the account.
        if ((call.callId ?? "").length === 0 || Session.phase === Session.SignedOut || Session.phase === Session.DeviceCode)
            return
        handledRing = call.callId
        joinAnswered(call)
    }
    Connections {
        target: Native
        function onCallAnswered() { window.takeAnswered() }
    }
    // What the call notification shows, and where its Mute starts from.
    // Sent once per update of the core's: its properties change one after
    // another (a deafen mutes too), and the shim must not see the half-way
    // state as a change the user made.
    readonly property string callState: Voice.channelId + "\n" + Voice.channelName + "\n" + Voice.muted + Voice.deafened
    onCallStateChanged: Qt.callLater(window.sendCallState)
    function sendCallState() {
        Native.setCallState(Voice.channelName, Voice.muted, Voice.deafened)
    }

    // Close what is on top, or everything down to the pager.
    function closeTop(all) {
        if (stack.depth < 2)
            return
        stack.sheet = stack.currentItem.sheet
        if (all)
            stack.pop(null)
        else
            stack.pop()
    }

    // Back, in order: a sheet or stacked page, then what is over the chat
    // (its image viewer), the pager's pages (members, chat), then out of
    // the app from the drawer.
    function goBack() {
        if (!ready) {
            Native.leave()
            return
        }
        // A join in flight holds the landing.
        if (stack.currentItem?.holdsBack === true) {
            stack.currentItem.backProgress = 0
            return
        }
        if (stack.depth > 1)
            closeTop(false)
        else if (!pager.back())
            Native.leave()
    }

    // Signed out (here or by the server): nothing stays stacked for the
    // next account.
    onReadyChanged: {
        if (ready) {
            openFromNotification()
            openFromLink()
            takeAnswered()
            openAnswered()
            return
        }
        dropAnswered()
        callWanted = false
        stack.pop(null, StackView.Immediate)
        pager.snap()
        if (Session.phase === Session.SignedOut)
            Native.forgetNotifications()
    }

    Item {
        id: content
        anchors.fill: parent
        // A freeform window's caption bar pushes the window down without
        // shrinking it (Native.clippedBottom): the pages end above the cut.
        anchors.bottomMargin: Native.clippedBottom

        Loader {
            anchors.fill: parent
            active: Session.phase === Session.Boot || Session.phase === Session.Loading
            sourceComponent: Item {
                Spinner {
                    anchors.centerIn: parent
                    running: true
                }
            }
        }

        Loader {
            anchors.fill: parent
            active: Session.phase === Session.SignedOut || Session.phase === Session.DeviceCode
            sourceComponent: SignInPage {}
        }

        StackView {
            id: stack
            anchors.fill: parent
            visible: window.ready
            enabled: visible

            // A sheet rises from the bottom edge and drops back; a page
            // slides and fades in from the right (Android's default
            // activity transition; iOS would push from the right edge).
            // Set before each push and pop, for the item that moves.
            property bool sheet: false

            initialItem: Pager {
                id: pager
                onProfileRequested: userId => window.openProfile(userId)
                onSettingsRequested: window.openSettings()
                onFriendsRequested: window.openFriends()
                onVoiceRequested: channelId => window.joinVoice(channelId)
                onCallRequested: window.openCall()
                onDmCallRequested: action => window.dmCallAction(action)
                onThreadRequested: messageId => window.openThread(messageId)
                onServerMenuRequested: serverSheet.openServer()
                onChannelMenuRequested: (channelId, name, voice) => serverSheet.openChannel(channelId, name, voice)
            }

            pushEnter: Transition {
                ParallelAnimation {
                    NumberAnimation { property: "x"; from: stack.sheet ? 0 : stack.width * 0.25; to: 0; duration: Theme.durSheet; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeDrawer }
                    NumberAnimation { property: "y"; from: stack.sheet ? stack.height : 0; to: 0; duration: Theme.durSheet; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeDrawer }
                    NumberAnimation { property: "opacity"; from: stack.sheet ? 1 : 0; to: 1; duration: Theme.durSheet; easing.type: Easing.OutCubic }
                }
            }
            pushExit: Transition {
                NumberAnimation { property: "opacity"; from: 1; to: 0.6; duration: Theme.durSheet }
            }
            popEnter: Transition {
                NumberAnimation { property: "opacity"; from: 0.6; to: 1; duration: Theme.durSheet }
            }
            popExit: Transition {
                ParallelAnimation {
                    NumberAnimation { property: "x"; to: stack.sheet ? 0 : stack.width * 0.25; duration: Theme.dur4Exit; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
                    NumberAnimation { property: "y"; to: stack.sheet ? stack.height : 0; duration: Theme.dur4Exit; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
                    NumberAnimation { property: "opacity"; to: stack.sheet ? 1 : 0; duration: Theme.dur4Exit; easing.type: Easing.OutCubic }
                }
            }
        }

        Component {
            id: profilePage
            ProfilePage {
                onCloseRequested: window.closeTop(false)
                onEditRequested: window.openProfileEditor()
                onMessageRequested: userId => {
                    // Back to the pager; the chat slides in once the DM
                    // (made now if they never talked) is the open one.
                    window.closeTop(true)
                    window.openUnseen(() => Nav.openDm(userId))
                    pager.showDmWhenOpen(userId)
                }
            }
        }
        Component {
            id: callPage
            CallPage {
                id: callItem
                onCloseRequested: {
                    // Only while it is the page on top (its call can end
                    // under a profile opened over it).
                    if (stack.currentItem === callItem)
                        window.closeTop(false)
                }
            }
        }
        Component {
            id: settingsPage
            SettingsPage {
                onCloseRequested: window.closeTop(false)
                onProfileEditRequested: window.openProfileEditor()
            }
        }
        Component {
            id: profileEditPage
            ProfileEditPage {
                onCloseRequested: window.closeTop(false)
            }
        }
        Component {
            id: threadPage
            ThreadPage {
                onCloseRequested: window.closeTop(false)
                onProfileRequested: userId => window.openProfile(userId)
            }
        }
        Component {
            id: landingPage
            InviteLandingPage {
                id: landingItem
                onCloseRequested: {
                    if (stack.currentItem === landingItem)
                        window.closeTop(false)
                }
                // Back to the drawer, on the server's tree.
                onServerOpened: {
                    window.closeTop(true)
                    pager.show("drawer")
                }
            }
        }
        Component {
            id: friendsPage
            FriendsPage {
                onCloseRequested: window.closeTop(false)
                onProfileRequested: userId => window.openProfile(userId)
                onMessageRequested: userId => {
                    window.closeTop(true)
                    window.openUnseen(() => Nav.openDm(userId))
                    pager.showDmWhenOpen(userId)
                }
            }
        }
    }

    // The open server's menu and a channel's: Invite people (through the
    // share sheet), Mark as read, a channel's Copy link.
    ServerSheet {
        id: serverSheet
        anchors.fill: parent
        anchors.bottomMargin: Native.clippedBottom
    }

    // The microphone was refused: say what it is for, and where to allow it
    // (after a second refusal Android no longer shows its dialog).
    ConfirmSheet {
        id: microphoneSheet
        anchors.fill: parent
        anchors.bottomMargin: Native.clippedBottom
        title: qsTr("Yap needs the microphone")
        body: qsTr("Voice channels use your microphone while you are in a call. Allow it for Yap in the system settings, then join again.")
        confirmText: qsTr("Open settings")
        onConfirmed: Native.openAppSettings()
    }

    // Ring them? (StartCallModal.tsx): a call rings at once, and leaves a
    // voice channel's call we are in.
    ConfirmSheet {
        id: callConfirm
        property string channelId: ""
        property string name: ""
        property string leaves: ""
        anchors.fill: parent
        anchors.bottomMargin: Native.clippedBottom
        title: qsTr("Call %1?").arg(name)
        body: leaves.length > 0 ? qsTr("This rings %1 right away, and you leave %2.").arg(name).arg(leaves)
            : qsTr("This rings %1 right away.").arg(name)
        confirmText: qsTr("Start call")
        onConfirmed: {
            const channelId = callConfirm.channelId
            window.withMicrophone(() => {
                Calls.start(channelId)
                window.openCall()
            })
        }
    }

    // A DM call ringing us: over everything, the lock screen included.
    IncomingCallSheet {
        anchors.fill: parent
        call: window.shownRing
        onAnswerRequested: window.answerRing()
        onDeclineRequested: window.declineRing()
    }

    // The system back gesture and its predictive preview, through Native's
    // shim: the progress drives whichever is on top.
    Connections {
        target: Native
        function onBackProgressed(progress) {
            if (!window.ready)
                return
            if (stack.depth > 1)
                stack.currentItem.backProgress = progress
            else
                pager.backPreview(progress)
        }
        function onBackCancelled() {
            if (!window.ready)
                return
            if (stack.depth > 1)
                stack.currentItem.backProgress = 0
            else
                pager.backCancel()
        }
        function onBackInvoked() {
            window.goBack()
        }
    }
    // True only while something is above the drawer: at the root the system
    // plays its own back-to-home preview. Where it would finish the activity
    // instead (not started by the launcher, Android 14+), the shim still
    // takes Back and goBack() leaves by moving the task back.
    Binding {
        target: Native
        property: "handlesBack"
        value: window.ready && (stack.depth > 1 || pager.canGoBack)
    }
}
