import QtQuick.Layouts
import com.yap.native

// The signed-in shell: server rail, channel tree, open channel.
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
    ChatView {
        Layout.fillHeight: true
        Layout.fillWidth: true
    }
}
