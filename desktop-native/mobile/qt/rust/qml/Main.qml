import QtQuick
import QtQuick.Controls.Basic
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The mobile shell: one full-screen window drawn edge to edge (the system
// bars float over it; SafeArea keeps content clear of them), the desktop's
// sign-in phases, and the navigation: Home's pager (drawer, chat, members)
// with pages and sheets stacked over it (a profile, Settings, Friends, the
// call), and the platform's back gesture stepping through them. The pages
// are per platform; the rows inside them are the desktop's.
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
    Connections {
        target: Native
        function onMicrophoneAnswered(granted) {
            const channelId = window.wantedVoice
            window.wantedVoice = ""
            if (channelId.length === 0 || !window.ready)
                return
            if (!granted) {
                microphoneSheet.open()
                return
            }
            Native.haptic(Native.Confirm)
            Voice.join(channelId)
            window.openCall()
        }
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
            return
        }
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
