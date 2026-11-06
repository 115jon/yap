import QtQuick
import com.yap.native

// A voice channel's status under its row (ChannelSidebar.tsx): the text in
// 12/500 ink-2 and the picture (at most 96 x 64), indented to the name
// (pl-7). Once we are connected there it is ours to change: "Set status"
// (or "Edit status") and the picture key open the status dialogs.
Column {
    id: root

    property string channelId
    property string channelName
    property string text
    property string media
    property bool editable: false

    leftPadding: 28
    topPadding: 2
    bottomPadding: 4
    spacing: 4

    Text {
        visible: root.text.length > 0
        width: root.width - root.leftPadding - 8
        leftPadding: 6
        elide: Text.ElideRight
        textFormat: Text.PlainText
        text: root.text
        color: Theme.ink2
        font: Theme.type.captionStrong
    }

    AnimatedImage {
        visible: root.media.length > 0 && status !== Image.Error
        width: Math.min(96, implicitWidth > 0 ? implicitWidth * height / Math.max(1, implicitHeight) : 96)
        height: 64
        fillMode: Image.PreserveAspectFit
        horizontalAlignment: Image.AlignLeft
        source: root.media
        asynchronous: true
        playing: visible && !Theme.reducedMotion
        Accessible.name: qsTr("Channel picture")
    }

    Row {
        visible: root.editable
        spacing: 2
        StatusKey {
            glyph: "edit"
            label: root.text.length > 0 ? qsTr("Edit status") : qsTr("Set status")
            onClicked: ShellState.voiceStatusRequested(root.channelId, root.channelName, false, root.text)
        }
        StatusKey {
            glyph: "gif"
            label: root.media.length > 0 ? qsTr("Change picture") : qsTr("Add picture")
            onClicked: ShellState.voiceStatusRequested(root.channelId, root.channelName, true, root.text)
        }
    }

    component StatusKey: Rectangle {
        id: key
        property string glyph
        property string label
        signal clicked()
        width: keyRow.implicitWidth + 12
        height: 24
        radius: Theme.radius
        color: keyHover.hovered ? Theme.tint1 : "transparent"
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: label
        Keys.onReturnPressed: key.clicked()
        Keys.onSpacePressed: key.clicked()
        Row {
            id: keyRow
            anchors.centerIn: parent
            spacing: 4
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: key.glyph
                size: Theme.iconXs
                color: keyHover.hovered ? Theme.ink1 : Theme.ink3
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: key.label
                color: keyHover.hovered ? Theme.ink1 : Theme.ink3
                font: Theme.type.caption
            }
        }
        Rectangle {
            anchors.fill: parent
            visible: key.activeFocus
            color: "transparent"
            radius: Theme.radius
            border.width: 2
            border.color: Theme.focus
        }
        HoverHandler { id: keyHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: key.clicked() }
    }
}
