pragma ComponentBehavior: Bound
import QtQuick
import Yap.Ui

// UserPlatformIndicators.tsx: where someone is signed in, as glyphs 6 px
// apart in desktop, mobile, web order, each with its tooltip. They are
// metadata: muted ink whatever the status (presence lives on the dot),
// unless the caller gives `ink` (and `offlineInk` while offline).
// `platforms` is the comma-joined list the core sends.
Row {
    id: root

    property string platforms: ""
    property string presence: ""
    property int size: Theme.iconXs
    property color ink: "transparent"
    property color offlineInk: "transparent"

    readonly property var list: platforms.length > 0 ? platforms.split(",") : []
    readonly property var meta: ({
        desktop: { icon: "monitor", label: qsTr("Desktop") },
        mobile: { icon: "mobile", label: qsTr("Mobile") },
        web: { icon: "globe", label: qsTr("Web") }
    })
    readonly property color shown: presence === "offline" && offlineInk.a > 0 ? offlineInk
        : ink.a > 0 ? ink : Theme.ink3

    visible: list.length > 0
    spacing: 6
    Accessible.role: Accessible.StaticText
    Accessible.name: qsTr("Active on %1").arg(list.map(p => (meta[p]?.label ?? p).toLowerCase()).join(", "))

    Repeater {
        model: root.list
        Icon {
            id: glyph
            required property string modelData
            anchors.verticalCenter: parent ? parent.verticalCenter : undefined
            name: root.meta[modelData]?.icon ?? "globe"
            size: root.size
            color: root.shown
            HoverHandler { id: glyphHover }
            LazyPopup {
                when: glyphHover.hovered
                sourceComponent: Tooltip {
                    visible: glyphHover.hovered
                    text: root.meta[glyph.modelData]?.label ?? ""
                }
            }
        }
    }
}
