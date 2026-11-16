pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Server settings → Audit log on the phone (the desktop's AuditPage, one
// AuditCore): each entry as a sentence ("Native Spike updated role
// Moderator") under the actor's face, its details and reason, and how long
// ago; a tap shows the exact time and the target's ID. Type and Member
// filters open as sheets (over the entries loaded so far), and Load older
// entries asks the server for the next page.
AdminPage {
    id: root

    readonly property AuditCore core: AuditCore {
        admin: root.model
        live: root.onTop
    }
    readonly property var list: core.list
    // The entry showing its exact time and ID.
    property string openedId: ""

    title: qsTr("Audit log")

    Component.onCompleted: core.load()

    component FilterKey: KitButton {
        size: "sm"
        variant: "secondary"
        trailingIconName: "chevronDown"
    }

    Text {
        visible: root.core.loading
        width: parent.width
        topPadding: 24
        horizontalAlignment: Text.AlignHCenter
        textFormat: Text.PlainText
        text: qsTr("Loading the audit log")
        color: Theme.ink3
        font: Theme.type.description
    }
    Column {
        visible: root.core.failed
        width: parent.width
        spacing: 12
        AdminNote {
            text: qsTr("Audit log didn't load. %1").arg(root.core.error)
        }
        KitButton {
            variant: "secondary"
            text: qsTr("Try again")
            onClicked: root.core.load()
        }
    }

    Flow {
        visible: root.core.ready && root.core.all.length > 0
        width: parent.width
        spacing: 8
        FilterKey {
            text: root.core.kindFilter.length > 0 ? root.core.kindLabel : qsTr("Type")
            variant: root.core.kindFilter.length > 0 ? "primary" : "secondary"
            Accessible.name: qsTr("Type: %1").arg(root.core.kindLabel)
            onClicked: kindSheet.open()
        }
        FilterKey {
            text: root.core.actorFilter.length > 0 ? root.core.actorLabel : qsTr("Member")
            variant: root.core.actorFilter.length > 0 ? "primary" : "secondary"
            Accessible.name: qsTr("Member: %1").arg(root.core.actorLabel)
            onClicked: actorSheet.open()
        }
        KitButton {
            visible: root.core.filtering
            size: "sm"
            variant: "ghost"
            text: qsTr("Clear")
            onClicked: {
                Native.haptic(Native.Tick)
                root.core.clearFilters()
            }
        }
    }

    AdminSection {
        visible: root.core.ready
        title: qsTr("Recent activity")
        description: root.core.empty ? qsTr("No activity yet. Changes to roles, channels, invites and bans show here as they happen.")
            : root.list.length === 0 ? qsTr("Nothing loaded matches these filters.")
            : qsTr("Administrative actions taken in this server.")

        Repeater {
            model: root.list
            delegate: Item {
                id: entry
                required property var modelData
                readonly property var target: modelData.target
                readonly property bool opened: root.openedId === modelData.id

                width: parent ? parent.width : 0
                implicitHeight: body.implicitHeight + 24
                height: implicitHeight
                Accessible.role: Accessible.Button
                Accessible.name: root.core.sentence(modelData)
                Accessible.description: [entry.modelData.details.map(detail => root.core.detailSpoken(detail)).join(", "),
                    entry.modelData.reason.length > 0 ? qsTr("Reason: %1").arg(entry.modelData.reason) : "",
                    entry.modelData.when,
                    entry.opened && entry.target && entry.target.id.length > 0 ? qsTr("ID %1").arg(entry.target.id) : ""]
                    .filter(part => part.length > 0).join(". ")
                Accessible.onPressAction: root.openedId = entry.opened ? "" : entry.modelData.id

                Rectangle {
                    anchors.fill: parent
                    anchors.leftMargin: -8
                    anchors.rightMargin: -8
                    radius: Theme.radius
                    color: tap.pressed ? Theme.tint2 : "transparent"
                    EaseColor on color {}
                }
                Avatar {
                    id: face
                    y: 12
                    size: 32
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
                    spacing: 2

                    // "Actor verb Target": the target bold when named, an
                    // invite code in mono.
                    Flow {
                        width: parent.width
                        spacing: 4
                        Text {
                            width: Math.min(implicitWidth, body.width)
                            textFormat: Text.PlainText
                            text: entry.modelData.actor
                            wrapMode: Text.Wrap
                            color: Theme.ink1
                            font: Theme.type.occupant
                        }
                        Text {
                            width: Math.min(implicitWidth, body.width)
                            textFormat: Text.PlainText
                            text: entry.modelData.verb
                            wrapMode: Text.Wrap
                            color: Theme.ink2
                            font: Theme.type.description
                        }
                        Text {
                            visible: entry.target !== null
                            width: Math.min(implicitWidth, body.width)
                            textFormat: Text.PlainText
                            text: entry.target ? entry.target.label : ""
                            wrapMode: Text.WrapAnywhere
                            color: entry.target && entry.target.named ? Theme.ink1 : Theme.ink2
                            font: entry.target && entry.target.code ? Theme.type.code
                                : entry.target && entry.target.named ? Theme.type.occupant : Theme.type.description
                        }
                    }
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
                                width: body.width - (detail.modelData.type === "perm" ? 18 : 0)
                                wrapMode: Text.Wrap
                                textFormat: Text.PlainText
                                text: root.core.detailText(detail.modelData)
                                color: Theme.ink2
                                font: Theme.type.description
                            }
                        }
                    }
                    Text {
                        visible: entry.modelData.reason.length > 0
                        width: parent.width
                        textFormat: Text.PlainText
                        text: qsTr("Reason: %1").arg(entry.modelData.reason)
                        wrapMode: Text.Wrap
                        color: Theme.ink2
                        font: Theme.type.description
                    }
                    Text {
                        width: parent.width
                        topPadding: 2
                        textFormat: Text.PlainText
                        text: entry.opened ? entry.modelData.when : root.core.ago(entry.modelData.at)
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                    Text {
                        visible: entry.opened && entry.target !== null && entry.target.id.length > 0
                        width: parent.width
                        textFormat: Text.PlainText
                        text: entry.target ? qsTr("ID %1").arg(entry.target.id) : ""
                        wrapMode: Text.WrapAnywhere
                        color: Theme.ink3
                        font: Theme.type.code
                    }
                }
                TapHandler {
                    id: tap
                    onTapped: {
                        Native.haptic(Native.Tick)
                        root.openedId = entry.opened ? "" : entry.modelData.id
                    }
                }
            }
        }
    }

    Column {
        visible: root.core.ready && root.core.canLoadMore
        width: parent.width
        spacing: 8
        KitButton {
            variant: "secondary"
            text: qsTr("Load older entries")
            loading: root.core.loadingMore
            onClicked: root.core.loadMore()
        }
        AdminNote {
            text: root.core.moreError
        }
    }

    overlays: [
        BottomSheet {
            id: kindSheet
            anchors.fill: parent
            title: qsTr("Show entries about")
            contentHeight: kindColumn.implicitHeight

            function choose(key) {
                Native.haptic(Native.Tick)
                root.core.kindFilter = key
                close()
            }

            Column {
                id: kindColumn
                width: parent.width
                bottomPadding: 8
                SheetRow {
                    text: qsTr("All types")
                    iconName: root.core.kindFilter.length === 0 ? "check" : ""
                    hint: String(root.core.all.length)
                    Accessible.role: Accessible.RadioButton
                    Accessible.checkable: true
                    Accessible.checked: root.core.kindFilter.length === 0
                    onTriggered: kindSheet.choose("")
                }
                Repeater {
                    model: root.core.kinds
                    delegate: SheetRow {
                        required property var modelData
                        text: modelData.label
                        iconName: root.core.kindFilter === modelData.key ? "check" : ""
                        hint: String(modelData.count)
                        Accessible.role: Accessible.RadioButton
                        Accessible.checkable: true
                        Accessible.checked: root.core.kindFilter === modelData.key
                        onTriggered: kindSheet.choose(modelData.key)
                    }
                }
            }
        },
        BottomSheet {
            id: actorSheet
            anchors.fill: parent
            title: qsTr("Show entries by")
            contentHeight: actorColumn.implicitHeight
            // Everyone who acted in the pages loaded: it scrolls.
            flickable: actorFlick

            function choose(id) {
                Native.haptic(Native.Tick)
                root.core.actorFilter = id
                close()
            }

            Flickable {
                id: actorFlick
                anchors.fill: parent
                contentHeight: actorColumn.implicitHeight
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                Column {
                    id: actorColumn
                    width: parent.width
                    bottomPadding: 8
                    SheetRow {
                        text: qsTr("Everyone")
                        iconName: root.core.actorFilter.length === 0 ? "check" : ""
                        hint: String(root.core.all.length)
                        Accessible.role: Accessible.RadioButton
                        Accessible.checkable: true
                        Accessible.checked: root.core.actorFilter.length === 0
                        onTriggered: actorSheet.choose("")
                    }
                    Repeater {
                        model: root.core.actors
                        delegate: SheetRow {
                            required property var modelData
                            text: modelData.name
                            iconName: root.core.actorFilter === modelData.id ? "check" : ""
                            hint: String(modelData.count)
                            Accessible.role: Accessible.RadioButton
                            Accessible.checkable: true
                            Accessible.checked: root.core.actorFilter === modelData.id
                            onTriggered: actorSheet.choose(modelData.id)
                        }
                    }
                    Text {
                        x: 16
                        width: parent.width - 32
                        topPadding: 8
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        text: qsTr("People who acted in the entries loaded so far.")
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                }
            }
        }
    ]
}
