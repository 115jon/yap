pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// A profile's badges (ui/user-badges.tsx), in their fixed order: each a
// key around its 16-cell drawing (BadgeArt below), printed like Yappy: the
// body through the accent Bayer dither, the edge solid accent, details in
// ink-1 (cpp/ditherprovider.cpp `badge/...`, one still image per badge and
// inks, never a Canvas). They carry meaning, so they stay under a
// high-contrast preference (in ink-1) and each key has an accessible name.
//
// Never cut off: they wrap onto another line, or with `maxVisible` that
// many show and a "+N" key whose tip names the rest. Hover or focus shows a
// key's tip (name, description, "Earned Nov 5, 2026"); a click or Space
// toggles it. Nothing shows without badges.
//
// `badges` is the core's JSON (`[{id, name, description, earned, label}]`).
Flow {
    id: root

    property string badges: ""
    // sm 16, md 24, lg 32 px drawings (1, 1.5 and 2 px a cell).
    property string size: "md"
    // Show at most this many, the rest folded into "+N" (0: wrap all).
    property int maxVisible: 0

    readonly property var list: {
        if (badges.length === 0)
            return []
        try {
            const parsed = JSON.parse(badges)
            return Array.isArray(parsed) ? parsed : []
        } catch (e) {
            return []
        }
    }
    readonly property int px: size === "sm" ? 16 : size === "lg" ? 32 : 24
    readonly property bool folds: maxVisible > 0 && list.length > maxVisible
    // "+N" takes a slot, so one fewer badge shows beside it.
    readonly property var shown: folds ? list.slice(0, Math.max(1, maxVisible - 1)) : list
    readonly property var rest: folds ? list.slice(shown.length) : []

    visible: list.length > 0
    spacing: size === "lg" ? 6 : 4
    Accessible.role: Accessible.List
    Accessible.name: qsTr("Badges")

    Repeater {
        model: root.shown
        delegate: BadgeKey {
            required property var modelData
            badge: modelData
        }
    }
    MoreKey {
        visible: root.rest.length > 0
    }

    // The drawing alone. Decorative: the key names it.
    component BadgeArt: Image {
        id: art
        property string badgeId
        readonly property bool forcedColors: {
            const hints = Application.styleHints.accessibility
            return hints ? hints.contrastPreference === Qt.HighContrast : false
        }
        width: root.px
        height: root.px
        smooth: false
        source: "image://dither/badge/" + badgeId + "/"
            + Theme.hex(forcedColors ? Theme.ink1 : Theme.accentInk) + "/" + Theme.hex(Theme.ink1)
        Accessible.ignored: true
    }

    // The tip: the name in semibold, the description, the date at 70 %.
    component Tip: T.ToolTip {
        id: tip
        property var lines: []
        x: parent ? Math.round((parent.width - implicitWidth) / 2) : 0
        y: parent ? -implicitHeight - 6 : 0
        margins: 8
        topPadding: 6
        bottomPadding: 6
        leftPadding: 8
        rightPadding: 8
        implicitWidth: Math.min(240, implicitContentWidth + leftPadding + rightPadding)
        implicitHeight: implicitContentHeight + topPadding + bottomPadding
        closePolicy: T.Popup.NoAutoClose
        contentItem: Column {
            spacing: 2
            Repeater {
                model: tip.lines
                delegate: Text {
                    required property var modelData
                    required property int index
                    width: Math.min(implicitWidth, 240 - 16)
                    textFormat: Text.PlainText
                    text: modelData
                    wrapMode: Text.Wrap
                    color: Theme.ply0
                    opacity: index === 2 ? 0.7 : 1
                    font: index === 0 ? Theme.type.labelStrong : Theme.type.captionStrong
                    lineHeight: Theme.type.captionLine
                    lineHeightMode: Text.FixedHeight
                }
            }
        }
        background: Rectangle {
            objectName: "yapNoPressSink"
            radius: Theme.radius
            color: Theme.ink1
        }
        enter: PresenceIn { duration: Theme.dur2 }
        exit: PresenceOut { duration: Theme.durTipExit; toScale: 1 }
    }

    // One key: 2 px around the drawing, tint-1 on hover, the focus ring.
    component Key: T.AbstractButton {
        id: key
        property bool pinned: false
        readonly property bool tipShown: pinned || hovered || visualFocus
        implicitWidth: root.px + 4
        implicitHeight: root.px + 4
        padding: 2
        focusPolicy: Qt.StrongFocus
        onClicked: pinned = !pinned
        onActiveFocusChanged: if (!activeFocus) pinned = false
        onHoveredChanged: if (!hovered && !activeFocus) pinned = false
        background: Rectangle {
            radius: 2
            color: key.hovered ? Theme.tint1 : "transparent"
            Rectangle {
                anchors.fill: parent
                anchors.margins: -2
                visible: key.visualFocus
                color: "transparent"
                radius: Theme.radius
                border.width: 2
                border.color: Theme.focus
            }
        }
    }

    component BadgeKey: Key {
        id: badgeKey
        property var badge: ({})
        readonly property var lines: [badge.name ?? "", badge.description ?? "", badge.earned ?? ""]
            .filter(line => line.length > 0)
        Accessible.role: Accessible.ListItem
        Accessible.name: badge.label ?? ""
        Accessible.description: badge.earned ?? ""
        contentItem: BadgeArt { badgeId: badgeKey.badge.id ?? "" }
        LazyPopup {
            when: badgeKey.tipShown
            sourceComponent: Tip {
                visible: badgeKey.tipShown
                lines: badgeKey.lines
            }
        }
    }

    component MoreKey: Key {
        id: more
        readonly property var names: root.rest.map(badge => badge.name)
        Accessible.role: Accessible.ListItem
        Accessible.name: qsTr("%1 more: %2").arg(root.rest.length).arg(names.join(", "))
        contentItem: Text {
            width: root.px
            height: root.px
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            textFormat: Text.PlainText
            text: "+" + root.rest.length
            color: Theme.ink2
            font: Theme.type.numSm
        }
        LazyPopup {
            when: more.tipShown
            sourceComponent: Tip {
                visible: more.tipShown
                lines: [more.names.join(", ")]
            }
        }
    }
}
