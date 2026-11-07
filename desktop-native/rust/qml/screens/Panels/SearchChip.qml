import QtQuick
import com.yap.native

// A filter chip (SearchPanel.tsx FilterChip): 24 high, up to 220, tint-2,
// `key:` in ink-3, the person's 16 px picture or the channel glyph, the
// value at 12/500 in ink-1, and a 20 px remove key.
Rectangle {
    id: root

    // {id, key, value, person, avatar, initials, icon}
    required property var chip

    signal removeRequested()

    implicitWidth: Math.min(220, row.implicitWidth + 8 + 2)
    implicitHeight: Theme.keyXs
    radius: Theme.radius
    color: Theme.tint2
    opacity: 0
    Component.onCompleted: opacity = 1
    Behavior on opacity { NumberAnimation { duration: Theme.dur1 } }
    Accessible.role: Accessible.StaticText
    Accessible.name: root.chip.key + ": " + root.chip.value

    Row {
        id: row
        x: 8
        anchors.verticalCenter: parent.verticalCenter
        spacing: 4
        Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: root.chip.key + ":"
            color: Theme.ink3
            font: Theme.type.caption
        }
        Avatar {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.chip.person === true
            size: Theme.iconSm
            initials: root.chip.initials ?? ""
            source: root.chip.avatar ?? ""
        }
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            visible: (root.chip.icon ?? "").length > 0
            name: root.chip.icon ?? ""
            size: Theme.iconXs
            color: Theme.ink3
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, 220 - 8 - 2 - 20 - 8 - x)
            textFormat: Text.PlainText
            text: root.chip.value
            elide: Text.ElideRight
            color: Theme.ink1
            font: Theme.type.captionStrong
        }
        IconKey {
            anchors.verticalCenter: parent.verticalCenter
            iconName: "close"
            size: 20
            glyph: Theme.iconXs
            tone: "muted"
            tip: qsTr("Remove filter %1: %2").arg(root.chip.key).arg(root.chip.value)
            tipSide: "bottom"
            onClicked: root.removeRequested()
        }
    }
}
