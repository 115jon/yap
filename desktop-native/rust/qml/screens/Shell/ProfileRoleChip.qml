import QtQuick
import com.yap.native

// One role chip (ProfileCard.tsx ProfileRoleChips): a 24 px tint-1 chip
// with the role's 10 px dot (ink-3 when it has no colour) and its name at
// 12/500. `removable` adds the 20 px muted close key (Manage roles).
Rectangle {
    id: chip

    required property string name
    property string tone: ""
    property bool removable: false
    property real maxWidth: 300

    signal removeRequested()

    width: Math.min(chipRow.implicitWidth + 8 + (removable ? 2 : 10), maxWidth)
    height: 24
    radius: Theme.radius
    color: Theme.tint1

    Row {
        id: chipRow
        x: 8
        anchors.verticalCenter: parent.verticalCenter
        spacing: 6
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: 10
            height: 10
            radius: 5
            color: chip.tone.length > 0 ? chip.tone : Theme.ink3
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: chip.name
            color: Theme.ink1
            font: Theme.type.captionStrong
        }
        IconKey {
            visible: chip.removable
            anchors.verticalCenter: parent.verticalCenter
            size: 20
            glyph: Theme.iconSm
            iconName: "close"
            tone: "muted"
            tip: qsTr("Remove the %1 role").arg(chip.name)
            onClicked: chip.removeRequested()
        }
    }
}
