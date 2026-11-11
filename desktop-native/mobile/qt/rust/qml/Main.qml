import QtQuick
import QtQuick.Controls.Basic
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The mobile shell: one full-screen window drawn edge to edge (the system
// bars float over it; SafeArea keeps content clear of them), the desktop's
// sign-in phases, and the navigation: Home's pager (drawer, chat, members)
// with pages and sheets stacked over it, and the platform's back gesture
// stepping through them. The pages are per platform; the rows inside them
// are the desktop's.
Window {
    id: window

    visible: true
    // Edge to edge: the window draws under the status and navigation bars.
    flags: Qt.Window | Qt.ExpandedClientAreaHint
    color: Theme.ply0
    title: "Yap"

    // The system's light or dark mode picks the theme, live.
    Binding {
        target: Theme
        property: "themeId"
        value: Application.styleHints.colorScheme === Qt.ColorScheme.Light ? "light" : "dark"
    }
    Binding {
        target: Theme
        property: "reducedMotion"
        value: Native.reducedMotion
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
        if (stack.depth > 1)
            closeTop(false)
        else if (!pager.back())
            Native.leave()
    }

    // Signed out (here or by the server): nothing stays stacked for the
    // next account.
    onReadyChanged: {
        if (ready)
            return
        stack.pop(null, StackView.Immediate)
        pager.snap()
    }

    Item {
        id: content
        anchors.fill: parent

        Loader {
            anchors.fill: parent
            active: Session.phase === Session.Boot || Session.phase === Session.Loading
            sourceComponent: Item {
                Spinner {
                    anchors.centerIn: parent
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
                    Nav.openDm(userId)
                    pager.showDmWhenOpen(userId)
                }
            }
        }
        Component {
            id: settingsPage
            SettingsPage {
                onCloseRequested: window.closeTop(false)
            }
        }
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
