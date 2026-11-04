import QtQuick
import QtQuick.Layouts
import com.yap.native

// The signed-in shell: server rail, channel tree, then the open channel:
// its messages, or a voice channel's view.
RowLayout {
    spacing: 0

    ServerRail {
        Layout.fillHeight: true
        Layout.preferredWidth: Theme.rail
    }
    ChannelSidebar {
        Layout.fillHeight: true
        Layout.preferredWidth: Theme.treePanel
    }
    Item {
        Layout.fillHeight: true
        Layout.fillWidth: true

        ChatView {
            anchors.fill: parent
            visible: Nav.voicePanelId.length === 0
        }
        Loader {
            anchors.fill: parent
            active: Nav.voicePanelId.length > 0
            sourceComponent: VoiceView {}
        }
    }
}
