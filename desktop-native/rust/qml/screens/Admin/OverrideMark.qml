import QtQuick
import com.yap.native
import Yap.Ui

// An override's mark (ChannelPermissionsTab.tsx OverrideMark): a role's
// colour dot in a 24 px slot, or the member's 24 px avatar.
Item {
    id: root

    required property var row

    width: 24
    height: 24
    Accessible.ignored: true

    Rectangle {
        visible: root.row.targetType === "role"
        anchors.centerIn: parent
        width: 12
        height: 12
        radius: 6
        color: root.row.color.length > 0 ? root.row.color : Theme.ink3
    }
    Avatar {
        visible: root.row.targetType !== "role"
        size: 24
        initials: root.row.initials.length > 0 ? root.row.initials : root.row.name
        source: root.row.avatar
    }
}
