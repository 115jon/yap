pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// The three plans (TierCards.tsx): Yap, Yap Loud and Founder side by side
// from 720 px (stacked below), each a 2 px-cornered ply-2 card with a 16 px
// inset: the kicker in silkscreen (the plan in force says so, in accent),
// the name at 20/700, the price under it where the kicker is not it, the
// mark (Yappy for Yap, the Loud or Founder badge), the blurb, the features
// with checks, and the plan's key at the foot. The card of the plan in
// force has an accent border.
//
//   founder  "Founder · lifetime" on its card, "Included" on the others,
//            no keys anywhere
//   loud     its card is the plan; Manage, off until billing ships
//   free     Upgrade on the Loud card, off with "Coming soon" until
//            billing answers
//
// `plan` is `Settings.plan` parsed (the core's plan_json); the words and
// the upload sizes come from there.
Item {
    id: root

    property var plan: null
    readonly property var cards: plan ? plan.cards : []
    readonly property bool billing: plan ? plan.billingEnabled === true : false
    readonly property bool wide: width >= 720
    // Floored, so three always fit on one line of the Flow.
    readonly property real cardWidth: wide ? Math.floor((width - 24) / 3) : width
    // A row of cards is as tall as its tallest (the grid's stretch).
    readonly property real rowHeight: {
        let tallest = 0
        for (let i = 0; i < repeater.count; ++i) {
            const card = repeater.itemAt(i)
            if (card)
                tallest = Math.max(tallest, card.contentHeight)
        }
        return tallest
    }

    implicitHeight: wide ? rowHeight : stack.implicitHeight

    Flow {
        id: stack
        width: root.width
        spacing: 12

        Repeater {
            id: repeater
            model: root.cards
            delegate: Card {}
        }
    }

    component Card: Rectangle {
        id: card
        required property var modelData
        readonly property bool current: modelData.current === true
        readonly property real contentHeight: body.implicitHeight + 32
            + (action.visible ? action.implicitHeight + 16 : 0)

        width: root.cardWidth
        height: root.wide ? root.rowHeight : contentHeight
        radius: 2
        color: Theme.ply2
        border.width: 1
        border.color: current ? Theme.accentInk : Theme.line
        Accessible.role: Accessible.Grouping
        Accessible.name: modelData.label

        Column {
            id: body
            x: 16
            y: 16
            width: parent.width - 32
            spacing: 16

            // The kicker, the name and the price, with the mark.
            Item {
                width: parent.width
                height: Math.max(heading.implicitHeight, mark.height)
                Column {
                    id: heading
                    width: parent.width - mark.width - 12
                    spacing: 4
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: card.modelData.kicker
                        wrapMode: Text.Wrap
                        color: card.current ? Theme.accentInk : Theme.ink3
                        font: Theme.type.silk
                        lineHeight: Theme.type.silkLine
                        lineHeightMode: Text.FixedHeight
                    }
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: card.modelData.name
                        wrapMode: Text.Wrap
                        color: Theme.ink1
                        font: Theme.type.sectionTitle
                        lineHeight: Theme.type.sectionTitleLine
                        lineHeightMode: Text.FixedHeight
                        Accessible.role: Accessible.Heading
                    }
                    Text {
                        visible: card.modelData.showPrice === true
                        width: parent.width
                        textFormat: Text.PlainText
                        text: card.modelData.price
                        color: Theme.ink2
                        font: Theme.type.num
                    }
                }
                Item {
                    id: mark
                    anchors.right: parent.right
                    width: 32
                    height: 32
                    YapBark {
                        visible: card.modelData.tier === "free"
                        cell: 1
                    }
                    // The badge's drawing alone (BadgeArt size lg): the
                    // card names the plan.
                    Image {
                        visible: card.modelData.tier !== "free"
                        width: 32
                        height: 32
                        smooth: false
                        source: visible ? "image://dither/badge/" + card.modelData.tier + "/"
                            + Theme.hex(Theme.accentInk) + "/" + Theme.hex(Theme.ink1) : ""
                        Accessible.ignored: true
                    }
                }
            }

            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: card.modelData.blurb
                wrapMode: Text.Wrap
                color: Theme.ink2
                font: Theme.type.description
                lineHeight: Theme.type.descriptionLine
                lineHeightMode: Text.FixedHeight
            }

            Column {
                width: parent.width
                spacing: 8
                Accessible.role: Accessible.List
                Repeater {
                    model: card.modelData.features
                    delegate: Row {
                        id: feature
                        required property string modelData
                        width: parent.width
                        spacing: 8
                        Accessible.role: Accessible.ListItem
                        Accessible.name: modelData
                        Icon {
                            y: 1
                            name: "check"
                            size: Theme.iconSm
                            color: card.current ? Theme.accentInk : Theme.ink3
                        }
                        Text {
                            width: parent.width - Theme.iconSm - 8
                            textFormat: Text.PlainText
                            text: feature.modelData
                            wrapMode: Text.Wrap
                            color: Theme.ink1
                            font: Theme.type.description
                            lineHeight: Theme.type.descriptionLine
                            lineHeightMode: Text.FixedHeight
                        }
                    }
                }
            }
        }

        // Upgrade (free) or Manage (Loud), off with "Coming soon" until
        // billing is on; nothing for founders. At the card's foot, so the
        // keys line up across a row.
        Flow {
            id: action
            visible: card.modelData.action.length > 0
            x: 16
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 16
            width: parent.width - 32
            spacing: 12
            KitButton {
                variant: card.modelData.action === "upgrade" ? "primary" : "secondary"
                size: "md"
                text: card.modelData.action === "upgrade" ? qsTr("Upgrade") : qsTr("Manage")
                enabled: root.billing
                ground: Theme.ply2
            }
            Text {
                visible: !root.billing
                height: 40
                verticalAlignment: Text.AlignVCenter
                textFormat: Text.PlainText
                text: qsTr("Coming soon")
                color: Theme.ink3
                font: Theme.type.silk
            }
        }
    }
}
