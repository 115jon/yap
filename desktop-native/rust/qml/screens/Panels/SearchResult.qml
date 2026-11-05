pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// One hit, drawn like a message row (SearchResultItem.tsx): a 24 px
// picture, the name (role colour) with the time and a pin, the excerpt
// with its matches marked (up to four lines), then what it carries. The
// whole row jumps to the message; "Jump" shows on hover and focus.
Rectangle {
    id: root

    // {id, channelId, name, color, avatar, initials, time, when, pinned,
    //  runs: [{text, kind}], placeholder, pills, more, embeds, active}
    required property var result
    // The panel's ply (the marks are solid over it).
    property color ground: Theme.ply1

    signal jumpRequested()

    function escapeHtml(text) {
        return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
    }
    // Matches on accent tint, mentions in accent ink (HighlightedExcerpt).
    readonly property string excerpt: {
        const runs = root.result.runs ?? []
        const mark = Theme.solid(Theme.accentTint, root.ground)
        return runs.map(run => {
            const text = root.escapeHtml(run.text).replace(/\n/g, "<br>")
            if (run.kind === "match")
                return "<span style=\"background-color:" + mark + ";color:" + String(Theme.ink1) + "\">" + text + "</span>"
            if (run.kind === "mention")
                return "<span style=\"color:" + String(Theme.accentInk) + ";font-weight:500\">" + text + "</span>"
            return text
        }).join("")
    }

    implicitHeight: body.implicitHeight + 16
    radius: Theme.radius
    color: result.active ? Theme.tint2 : press.pressed ? Theme.tint2 : hover.hovered ? Theme.tint1 : "transparent"
    Behavior on color { ColorAnimation { duration: Theme.dur1 } }
    border.width: activeFocus ? 2 : 0
    border.color: Theme.focus
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: qsTr("Jump to message from %1, %2").arg(result.name).arg(result.when)
    Accessible.description: (result.runs ?? []).map(run => run.text).join("") || result.placeholder
    Accessible.onPressAction: root.jumpRequested()
    Keys.onReturnPressed: root.jumpRequested()
    Keys.onSpacePressed: root.jumpRequested()

    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { id: press; onTapped: root.jumpRequested() }

    Avatar {
        id: avatar
        x: 8
        y: 8
        size: Theme.iconXl
        initials: root.result.initials
        source: root.result.avatar
    }

    Column {
        id: body
        anchors.left: avatar.right
        anchors.leftMargin: 10
        anchors.right: parent.right
        anchors.rightMargin: 8
        y: 8
        spacing: 0

        Item {
            width: parent.width
            height: 20
            Row {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - 48
                spacing: 8
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.min(implicitWidth, parent.width - time.implicitWidth - 32)
                    textFormat: Text.PlainText
                    text: root.result.name
                    elide: Text.ElideRight
                    color: root.result.color.length > 0 ? root.result.color : Theme.ink1
                    font: Theme.type.occupant
                }
                Text {
                    id: time
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: root.result.time
                    color: Theme.ink3
                    font: Theme.type.num
                }
                Icon {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.result.pinned
                    name: "pin"
                    variant: "active"
                    size: Theme.iconXs
                    color: Theme.ink3
                }
            }
        }

        Text {
            width: parent.width
            textFormat: root.result.runs.length > 0 ? Text.RichText : Text.PlainText
            text: root.result.runs.length > 0 ? root.excerpt : root.result.placeholder
            wrapMode: Text.Wrap
            // line-clamp-4 (rich text does not elide).
            height: Math.min(implicitHeight, 4 * Theme.type.descriptionLine)
            clip: implicitHeight > height
            color: root.result.runs.length > 0 ? Theme.ink2 : Theme.ink3
            font: Theme.type.description
            lineHeight: Theme.type.descriptionLine
            lineHeightMode: Text.FixedHeight
        }

        Flow {
            width: parent.width
            topPadding: 6
            spacing: 4
            visible: root.result.pills.length > 0 || root.result.embeds > 0
            Repeater {
                model: root.result.pills
                delegate: SearchPill {
                    required property var modelData
                    icon: modelData.icon
                    label: modelData.name
                    spoken: modelData.kind + ": " + modelData.name
                }
            }
            SearchPill {
                visible: root.result.more > 0
                label: qsTr("+%1 more").arg(root.result.more)
                muted: true
            }
            SearchPill {
                visible: root.result.embeds > 0
                icon: "link"
                label: root.result.embeds === 1 ? qsTr("Link preview") : qsTr("%1 link previews").arg(root.result.embeds)
            }
        }
    }

    Rectangle {
        anchors.top: parent.top
        anchors.topMargin: 6
        anchors.right: parent.right
        anchors.rightMargin: 6
        width: jump.implicitWidth + 16
        height: Theme.keyXs
        radius: Theme.radius
        color: Theme.tint2
        opacity: hover.hovered || root.activeFocus ? 1 : 0
        Behavior on opacity { OpacityAnimator { duration: Theme.dur1 } }
        Accessible.ignored: true
        Text {
            id: jump
            anchors.centerIn: parent
            text: qsTr("Jump")
            color: Theme.ink1
            font: Theme.type.captionStrong
        }
    }
}
