import QtQuick
import com.yap.native
import Yap.Ui

// Pinned message (widgets/Quote.tsx): a snapshot of a message the owner
// sent, as it was when pinned. A viewer outside the message's server sees
// that it exists, not what it says. (The web's "Jump" to the message is
// not here yet: the native board has no navigation hook.)
WidgetPlate {
    id: root

    required property var widget
    required property var boardData
    required property var ctx

    readonly property var snapshot: widget.cfg?.snapshot ?? null
    readonly property bool pinned: snapshot !== null && !!(widget.cfg?.message_id)
    readonly property bool hidden: pinned && !(ctx?.isOwner ?? false) && root.boardData?.quote_visible === false
    readonly property real at: snapshot ? Date.parse(snapshot.at ?? "") : NaN
    readonly property string when: isFinite(at)
        ? Board.dayMonth(snapshot.at) + " · " + Qt.formatTime(new Date(at), "hh:mm")
        : ""
    readonly property string where: {
        if (!snapshot)
            return ""
        const parts = []
        if (snapshot.channel_name)
            parts.push("#" + snapshot.channel_name)
        if (snapshot.server_name)
            parts.push(snapshot.server_name)
        return parts.join(" · ")
    }
    readonly property real bodyWidth: root.body.width

    kind: widget.type
    size: widget.size
    draft: widget.draft ?? false
    // The web's readout is a filled pin glyph; the plate's readout is text.
    readout: ""

    WidgetEmpty {
        visible: !root.pinned
        icon: "pin"
        title: root.ctx?.isOwner ? qsTr("Nothing pinned yet") : qsTr("Nothing pinned")
        hint: root.ctx?.isOwner ? qsTr("Pin a message you sent from its menu or in Edit board.") : ""
    }

    WidgetEmpty {
        visible: root.hidden
        icon: "lock"
        title: qsTr("Message from a server you're not in")
        hint: root.size === "S" ? "" : qsTr("Only members of that server can read it.")
    }

    Item {
        visible: root.pinned && !root.hidden
        anchors.fill: parent

        // Byline: the owner's avatar and name, and when it was sent.
        Row {
            id: byline
            width: parent.width
            spacing: 8

            Avatar {
                anchors.verticalCenter: parent.verticalCenter
                size: 24
                initials: root.ctx?.ownerInitials ?? ""
                source: root.ctx?.ownerAvatar ?? ""
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, byline.width - 24 - byline.spacing
                    - (stamp.visible ? stamp.implicitWidth + byline.spacing : 0))
                textFormat: Text.PlainText
                text: root.ctx?.ownerName ?? ""
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.author
                lineHeight: Theme.type.authorLine
                lineHeightMode: Text.FixedHeight
            }
            Text {
                id: stamp
                visible: root.when.length > 0 && root.bodyWidth >= 200
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: root.when
                color: Theme.ink3
                font: Theme.type.num
            }
        }

        Text {
            id: quoteText
            anchors.top: byline.bottom
            anchors.topMargin: 10
            width: parent.width
            height: Math.min(implicitHeight, footer.y - 10 - y)
            // The message grammar as rich text (markdown.rs), as the bio
            // renders it; plain text from an older core.
            readonly property string html: root.snapshot?.html ?? ""
            textFormat: html.length > 0 ? Text.RichText : Text.PlainText
            text: html.length > 0 ? Theme.richText(html, false, false) : (root.snapshot?.text ?? "")
            clip: true
            wrapMode: Text.Wrap
            maximumLineCount: root.size === "S" ? 3 : root.size === "M" ? 9 : 2
            elide: Text.ElideRight
            color: Theme.ink1
            font: root.size === "S" ? Theme.type.description : Theme.type.dialogBody
            lineHeight: root.size === "S" ? Theme.type.descriptionLine : Theme.type.dialogBodyLine
            lineHeightMode: Text.FixedHeight
            Accessible.role: Accessible.StaticText
            Accessible.name: root.snapshot?.text ?? ""
            HoverHandler { id: quoteHover }
            Tooltip {
                visible: quoteHover.hovered && quoteText.truncated
                text: root.snapshot?.text ?? ""
            }
        }

        // Where it was said.
        Text {
            id: footer
            anchors.bottom: parent.bottom
            width: parent.width
            textFormat: Text.PlainText
            text: root.where
            elide: Text.ElideRight
            color: Theme.ink3
            font: Theme.type.caption
        }
    }
}
