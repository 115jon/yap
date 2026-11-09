pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// Server settings → Audit log (AuditLogTab.tsx): entries as sentences
// ("Native Spike updated role Moderator") with targets by name (the id in
// a tooltip), permission changes as a + / − list, the reason and the local
// time. Never a raw id or bitfield in the text. The server sends the 50
// newest (the web asks for no more); rows build a page at a time as the
// page scrolls.
Column {
    id: root

    required property var host
    required property var model

    readonly property var audit: model.audit
    readonly property var list: audit.items ?? []
    readonly property var blank: ({ actor: "", verb: "", target: null, details: [], reason: "", when: "", actorInitials: "", actorAvatar: "" })

    spacing: 40
    Component.onCompleted: Admin.load(model.server.id, "audit")

    PageState {
        visible: root.audit.state === "error"
        title: qsTr("Audit log didn't load")
        message: root.audit.error
        failed: true
        onRetry: Admin.load(root.model.server.id, "audit")
    }
    LoadingRows {
        visible: root.audit.state === "idle" || root.audit.state === "loading"
        label: qsTr("Loading the audit log")
        rows: 3
    }
    PageState {
        visible: root.audit.state === "ready" && root.list.length === 0
        title: qsTr("No activity yet")
        message: qsTr("Changes to roles, channels, invites and bans show here as they happen.")
    }

    SettingsCard {
        visible: root.audit.state === "ready" && root.list.length > 0
        width: parent.width
        title: qsTr("Recent activity")
        description: qsTr("Administrative actions taken in this server.")

        PagedRows {
            total: root.list.length
            wanted: root.host.nearEnd
            delegate: Item {
                id: entry
                required property int index
                readonly property var modelData: root.list[index] ?? root.blank
                readonly property var target: modelData.target

                width: root.width
                implicitHeight: 12 + body.implicitHeight + 12
                Accessible.role: Accessible.ListItem
                Accessible.name: modelData.actor + " " + modelData.verb + (target ? " " + target.label : "")

                // AuditRow: px-4 py-3, gap-3, no rule between rows.
                Avatar {
                    id: face
                    x: 16
                    y: 12
                    size: 24
                    initials: entry.modelData.actorInitials
                    source: entry.modelData.actorAvatar
                    Accessible.ignored: true
                }
                Column {
                    id: body
                    y: 12
                    anchors.left: face.right
                    anchors.leftMargin: 12
                    anchors.right: parent.right
                    anchors.rightMargin: 16
                    // "Actor verb Target": the target bold when named, the
                    // invite code in mono, the id on hover.
                    Flow {
                        width: parent.width
                        spacing: 4
                        Text {
                            textFormat: Text.PlainText
                            text: entry.modelData.actor
                            color: Theme.ink1
                            font: Theme.type.occupant
                            lineHeight: Theme.type.descriptionLine
                            lineHeightMode: Text.FixedHeight
                        }
                        Text {
                            textFormat: Text.PlainText
                            text: entry.modelData.verb
                            color: Theme.ink2
                            font: Theme.type.description
                            lineHeight: Theme.type.descriptionLine
                            lineHeightMode: Text.FixedHeight
                        }
                        Text {
                            id: targetText
                            visible: entry.target !== null
                            textFormat: Text.PlainText
                            text: entry.target ? entry.target.label : ""
                            color: entry.target && entry.target.named ? Theme.ink1 : Theme.ink2
                            font: entry.target && entry.target.code ? Theme.type.code
                                : entry.target && entry.target.named ? Theme.type.occupant : Theme.type.description
                            lineHeight: Theme.type.descriptionLine
                            lineHeightMode: Text.FixedHeight
                            HoverHandler { id: targetHover; enabled: entry.target !== null && entry.target.id.length > 0 }
                            LazyPopup {
                                when: targetHover.hovered
                                sourceComponent: Tooltip {
                                    visible: targetHover.hovered
                                    text: entry.target ? qsTr("ID %1").arg(entry.target.id) : ""
                                }
                            }
                            // A dotted line-strong underline 3 under the
                            // baseline (decoration-dotted, offset 3 px).
                            Row {
                                visible: entry.target !== null && entry.target.id.length > 0
                                y: Math.round(targetText.baselineOffset) + 3
                                spacing: 1
                                Repeater {
                                    model: Math.ceil(targetText.width / 2)
                                    Rectangle { width: 1; height: 1; color: Theme.lineStrong }
                                }
                            }
                        }
                    }
                    // Details: mt-1.5, gap-0.5.
                    Column {
                        visible: entry.modelData.details.length > 0
                        width: parent.width
                        topPadding: 6
                        spacing: 2
                    Repeater {
                        model: entry.modelData.details
                        delegate: Row {
                            id: detail
                            required property var modelData
                            spacing: 6
                            Text {
                                visible: detail.modelData.type === "perm"
                                width: 12
                                horizontalAlignment: Text.AlignHCenter
                                textFormat: Text.PlainText
                                text: detail.modelData.sign ?? ""
                                color: detail.modelData.sign === "+" ? Theme.successInk : Theme.dangerInk
                                font: Theme.type.occupant
                                Accessible.name: detail.modelData.sign === "+" ? qsTr("Allowed") : qsTr("Removed")
                            }
                            Text {
                                width: body.width - 18
                                wrapMode: Text.Wrap
                                color: Theme.ink2
                                font: Theme.type.description
                                lineHeight: Theme.type.descriptionLine
                                lineHeightMode: Text.FixedHeight
                                textFormat: Text.PlainText
                                text: detail.modelData.type === "perm" ? detail.modelData.name
                                    : detail.modelData.type === "list" ? detail.modelData.label + ": " + detail.modelData.items.join(", ")
                                    : detail.modelData.text
                            }
                        }
                    }
                    }
                    Text {
                        visible: entry.modelData.reason.length > 0
                        width: parent.width
                        topPadding: 6
                        lineHeight: Theme.type.descriptionLine
                        lineHeightMode: Text.FixedHeight
                        textFormat: Text.PlainText
                        text: qsTr("Reason: %1").arg(entry.modelData.reason)
                        wrapMode: Text.Wrap
                        color: Theme.ink2
                        font: Theme.type.description
                    }
                    Text {
                        topPadding: 4
                        textFormat: Text.PlainText
                        text: entry.modelData.when
                        color: Theme.ink3
                        font: Theme.type.caption
                        lineHeight: Theme.type.captionLine
                        lineHeightMode: Text.FixedHeight
                    }
                }
            }
        }
    }
}
