pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// The member panel (MemberList.tsx members tab): role groups and Offline as
// tree branches, px-2, rows 2 px apart. Docked it is a 240 px ply-0 column
// with a seam on its left; below 1024 px the shell shows it as a sheet.
// A row opens the member's profile to the panel's left.
Item {
    id: root

    property string profileSide: "left"
    // The ply behind the rows (the presence notch is cut out of it).
    property color ground: Theme.ply0

    ListView {
        id: list
        anchors.fill: parent
        anchors.leftMargin: 8
        anchors.rightMargin: 8
        bottomMargin: 40
        spacing: 2
        clip: true
        model: MemberListModel
        boundsBehavior: Flickable.StopAtBounds
        reuseItems: true

        delegate: MemberEntry {
            width: list.width
            ground: root.ground
            onProfileRequested: (userId, anchor) => ShellState.showProfile(userId, anchor, root.profileSide)
            onMenuRequested: (userId, name, anchor, at) => ShellState.memberMenuRequested(userId, name, anchor, at)
        }

        ScrollBar.vertical: ScrollBar {
            policy: ScrollBar.AsNeeded
            contentItem: Rectangle {
                implicitWidth: 6
                radius: 3
                color: Theme.lineStrong
            }
        }
    }

    // EmptyState sm: nothing loaded for this server yet.
    Column {
        anchors.centerIn: parent
        width: parent.width - 32
        spacing: 4
        visible: list.count === 0
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: qsTr("No members to show")
            color: Theme.ink1
            font: Theme.type.panelTitle
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            text: qsTr("People who join this server appear here.")
            color: Theme.ink3
            font: Theme.type.description
        }
    }
}
