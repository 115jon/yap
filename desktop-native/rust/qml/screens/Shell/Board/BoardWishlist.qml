pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// The Wishlist tab (ProfileWishlist.tsx), a hint list: shop collectibles by
// name and free-text hints. Others see it read-only; the owner adds a hint
// (up to 24 items, 80 characters each) and removes items, saved at once.
// Adding shop items from the catalog is done from the web for now.
Column {
    id: root

    property var view: ({})
    property string ownerName: ""
    readonly property bool owner: view?.isOwner ?? false
    readonly property var items: view?.items ?? []

    function save(next) {
        Board.saveWishlist(JSON.stringify(next))
    }

    spacing: 12

    Text {
        visible: (root.view?.loading ?? false) && !(root.view?.loaded ?? false)
        text: qsTr("Loading…")
        color: Theme.ink3
        font: Theme.type.description
    }
    Column {
        visible: (root.view?.error ?? "").length > 0
        width: parent.width
        spacing: 8
        Text {
            width: parent.width
            text: root.view?.error ?? ""
            wrapMode: Text.Wrap
            color: Theme.ink2
            font: Theme.type.description
        }
        KitButton {
            visible: !root.owner || root.items.length === 0
            size: "sm"
            variant: "secondary"
            iconName: "refresh"
            text: qsTr("Try again")
            onClicked: Board.retry()
        }
    }
    Text {
        visible: (root.view?.loaded ?? false) && root.items.length === 0
        width: parent.width
        text: root.owner ? qsTr("Hint at what you'd like: shop items or anything else. Everyone who opens your profile can see it.")
            : qsTr("%1 hasn't added anything to their wishlist yet.").arg(root.ownerName)
        wrapMode: Text.Wrap
        color: Theme.ink3
        font: Theme.type.description
    }

    Grid {
        width: parent.width
        columns: 2
        columnSpacing: 14
        rowSpacing: 10
        Repeater {
            model: root.items
            delegate: Rectangle {
                id: item
                required property var modelData
                required property int index
                width: Math.floor((root.width - 14) / 2)
                height: 64
                radius: Theme.radius
                color: Theme.ply2
                border.width: 1
                border.color: Theme.lineStrong
                Accessible.role: Accessible.ListItem
                Accessible.name: modelData.kind === "shop" ? modelData.name : modelData.text
                HoverHandler { id: itemHover }
                Rectangle {
                    x: 12
                    anchors.verticalCenter: parent.verticalCenter
                    width: 36
                    height: 36
                    radius: Theme.radius
                    color: Theme.tint2
                    Icon {
                        anchors.centerIn: parent
                        name: item.modelData.kind === "shop" ? "shop" : "lightbulb"
                        size: Theme.iconSm
                        color: Theme.ink2
                    }
                }
                Column {
                    x: 60
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - 60 - 40
                    spacing: 2
                    Text {
                        text: item.modelData.kind === "shop" ? qsTr("Shop") : qsTr("Hint")
                        color: Theme.ink3
                        font: Theme.type.silk
                    }
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: item.modelData.kind === "shop" ? item.modelData.name : item.modelData.text
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                        color: Theme.ink1
                        font: Theme.type.description
                    }
                }
                IconKey {
                    visible: root.owner && (itemHover.hovered || hovered)
                    anchors.right: parent.right
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    size: 24
                    iconName: "trash"
                    tone: "destructive"
                    tip: qsTr("Remove")
                    onClicked: {
                        const next = root.items.slice()
                        next.splice(item.index, 1)
                        root.save(next)
                    }
                }
            }
        }
    }

    Row {
        visible: root.owner && (root.view?.loaded ?? false) && root.items.length < (root.view?.max ?? 24)
        width: parent.width
        spacing: 8
        KitField {
            id: hint
            width: parent.width - addKey.width - 8
            small: true
            maximumLength: 80
            placeholderText: qsTr("Add a hint, like \"anything with cats\"")
            Accessible.name: qsTr("Add a hint")
            onAccepted: addKey.clicked()
        }
        KitButton {
            id: addKey
            size: "sm"
            variant: "secondary"
            iconName: "add"
            text: qsTr("Add")
            enabled: hint.text.trim().length > 0 && !(root.view?.saving ?? false)
            onClicked: {
                const text = hint.text.trim()
                if (text.length === 0)
                    return
                root.save(root.items.concat([{ id: "h" + Date.now().toString(36), kind: "hint", text: text }]))
                hint.text = ""
            }
        }
    }
}
