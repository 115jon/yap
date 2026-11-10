import QtQuick
import QtQuick.Controls.Basic
import Yap.Mobile
import com.yap.native

// The mobile shell: one full-screen window drawn edge to edge (the system
// bars float over it; SafeArea keeps content clear of them), a stack of
// pages with the platform's back gesture, and the desktop's sign-in phases.
// The pages are per platform; the rows inside them are the desktop's.
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

    Component.onCompleted: Session.start()

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
            visible: Session.phase === Session.Ready
            enabled: visible
            initialItem: ServersPage {
                onChannelOpened: stack.push(chatPage)
            }

            // Android's slide-and-fade between pages (the system default
            // activity transition); iOS would push from the right edge.
            pushEnter: Transition {
                ParallelAnimation {
                    NumberAnimation { property: "x"; from: stack.width * 0.25; to: 0; duration: Theme.durSheet; easing.type: Easing.OutCubic }
                    NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.durSheet; easing.type: Easing.OutCubic }
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
                    NumberAnimation { property: "x"; to: stack.width * 0.25; duration: Theme.durSheet; easing.type: Easing.InCubic }
                    NumberAnimation { property: "opacity"; to: 0; duration: Theme.durSheet; easing.type: Easing.InCubic }
                }
            }
        }

        Component {
            id: chatPage
            ChatPage {
                onBack: stack.pop()
            }
        }
    }

    // The system back gesture (and the predictive back preview, through
    // Native's shim): close the chat page, else leave the app as usual.
    Connections {
        target: Native
        function onBackProgressed(progress) {
            if (stack.depth > 1)
                stack.currentItem.backProgress = progress
        }
        function onBackCancelled() {
            if (stack.depth > 1)
                stack.currentItem.backProgress = 0
        }
        function onBackInvoked() {
            if (stack.depth > 1)
                stack.pop()
            else
                Native.leave()
        }
    }
    Binding {
        target: Native
        property: "handlesBack"
        value: stack.depth > 1
    }
}
