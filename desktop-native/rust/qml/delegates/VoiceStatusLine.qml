import QtQuick
import com.yap.native

// A voice channel's status after its people (ChannelSidebar.tsx), indented
// to the name (pl-7), 2 over and 4 under. Once we are connected there it is
// ours to change: the status line reads the status (ink-2) or "Set a
// channel status" (ink-3) with the edit glyph after it, and the picture
// key reads "Set the vibe" until there is one. Anyone else sees the
// status and the picture only.
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
        visible: !root.editable && root.text.length > 0
        width: root.width - root.leftPadding - 8
        height: 24
        leftPadding: 6
        rightPadding: 6
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
        textFormat: Text.PlainText
        text: root.text
        color: Theme.ink2
        font: Theme.type.captionStrong
    }

    StatusKey {
        visible: root.editable
        maxWidth: root.width - root.leftPadding - 8
        label: root.text.length > 0 ? root.text : qsTr("Set a channel status")
        labelInk: root.text.length > 0 ? Theme.ink2 : Theme.ink3
        trailingGlyph: "edit"
        tip: root.text.length > 0 ? qsTr("Edit status") : qsTr("Set status")
        onClicked: ShellState.voiceStatusRequested(root.channelId, root.channelName, false, root.text)
    }

    AnimatedImage {
        id: picture
        visible: root.media.length > 0 && status !== Image.Error
        width: Math.min(96, implicitWidth > 0 ? implicitWidth * height / Math.max(1, implicitHeight) : 96)
        height: 64
        fillMode: Image.PreserveAspectFit
        horizontalAlignment: Image.AlignLeft
        source: root.media
        asynchronous: true
        playing: visible && !Theme.reducedMotion
        Accessible.name: qsTr("Channel picture")
        HoverHandler { enabled: root.editable; cursorShape: Qt.PointingHandCursor }
        TapHandler {
            enabled: root.editable
            onTapped: ShellState.voiceStatusRequested(root.channelId, root.channelName, true, root.text)
        }
    }

    StatusKey {
        visible: root.editable && root.media.length === 0
        maxWidth: root.width - root.leftPadding - 8
        leadingGlyph: "sparkle"
        label: qsTr("Set the vibe")
        labelInk: Theme.ink3
        onClicked: ShellState.voiceStatusRequested(root.channelId, root.channelName, true, root.text)
    }

    // h-6 px-1.5 gap-1.5, 12/500; hover tint-1, the glyph and the quiet
    // label go ink-1.
    component StatusKey: Rectangle {
        id: key
        property string label
        property color labelInk: Theme.ink3
        property string leadingGlyph: ""
        property string trailingGlyph: ""
        property string tip: ""
        property real maxWidth: 200
        signal clicked()
        width: Math.min(maxWidth, keyRow.implicitWidth + 12)
        height: 24
        radius: Theme.radius
        color: keyHover.hovered ? Theme.tint1 : "transparent"
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: tip.length > 0 ? tip + ": " + label : label
        Keys.onReturnPressed: key.clicked()
        Keys.onSpacePressed: key.clicked()
        Row {
            id: keyRow
            x: 6
            width: Math.min(implicitWidth, key.maxWidth - 12)
            anchors.verticalCenter: parent.verticalCenter
            spacing: 6
            Icon {
                visible: key.leadingGlyph.length > 0
                anchors.verticalCenter: parent.verticalCenter
                name: key.leadingGlyph || "sparkle"
                size: Theme.iconXs
                color: keyHover.hovered ? Theme.ink1 : Theme.ink3
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, key.maxWidth - 12 - (key.leadingGlyph.length > 0 || key.trailingGlyph.length > 0 ? 18 : 0))
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: key.label
                color: key.leadingGlyph.length > 0 && keyHover.hovered ? Theme.ink1 : key.labelInk
                font: Theme.type.captionStrong
            }
            Icon {
                visible: key.trailingGlyph.length > 0
                anchors.verticalCenter: parent.verticalCenter
                name: key.trailingGlyph || "edit"
                size: Theme.iconXs
                color: keyHover.hovered ? Theme.ink1 : Theme.ink3
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
        Tooltip {
            visible: keyHover.hovered && key.tip.length > 0
            text: key.tip
        }
        HoverHandler { id: keyHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: key.clicked() }
    }
}
