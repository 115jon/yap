pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// Choose an image (AvatarPickerModal.tsx): "Upload image" (from this
// device), then Recent avatars, the last six uploads
// (`/api/avatars/history`) as 56 px tiles; the current one wears the
// accent ring, the others a delete key on hover (`DELETE
// /api/avatars/history {id}`). A tile is picked for the editor, which
// saves it with the rest of the profile. Cancel closes it.
PlateDialog {
    id: dialog

    property string name
    readonly property var avatars: JSON.parse(Extras.avatars)
    readonly property bool busy: avatars.busyId.length > 0

    signal uploadRequested()
    signal recentPicked(string stored, string url)

    title: qsTr("Choose an image")
    description: qsTr("Upload a new avatar or reuse one of your recent uploads.")
    onAboutToShow: Extras.loadAvatars()

    AbstractButton {
        id: upload
        HoverHandler { cursorShape: Qt.PointingHandCursor }
        width: parent.width
        height: 112
        hoverEnabled: true
        enabled: !dialog.busy
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Upload image")
        onClicked: {
            dialog.close()
            dialog.uploadRequested()
        }
        contentItem: Column {
            spacing: 4
            Item { width: 1; height: 20 }
            Icon {
                anchors.horizontalCenter: parent.horizontalCenter
                name: "image"
                size: Theme.iconXl
                color: Theme.ink2
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Upload image")
                color: Theme.ink1
                font: Theme.type.row
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("From your device")
                color: Theme.ink2
                font: Theme.type.caption
            }
        }
        background: Rectangle {
            radius: Theme.radiusSheet
            color: upload.hovered ? Theme.tint2 : Theme.tint1
            EaseColor on color {}
            border.width: upload.visualFocus ? 2 : 1
            border.color: upload.visualFocus ? Theme.focus : Theme.line
        }
        scale: down ? Theme.scalePress : 1
    }

    Column {
        width: parent.width
        spacing: 12
        Item {
            width: parent.width
            height: heading.implicitHeight
            Column {
                id: heading
                spacing: 2
                Text {
                    text: qsTr("Recent avatars")
                    color: Theme.ink1
                    font: Theme.type.panelTitle
                    Accessible.role: Accessible.Heading
                }
                Text {
                    text: qsTr("Your last 6 uploads.")
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
            Icon {
                id: spinner
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                visible: dialog.avatars.loading
                name: "loader"
                size: Theme.iconSm
                color: Theme.ink3
                Accessible.name: qsTr("Loading")
                RotationAnimator on rotation {
                    from: 0
                    to: 360
                    duration: Theme.durSpin
                    loops: Animation.Infinite
                    running: spinner.visible && !Theme.reducedMotion
                }
            }
        }
        Flow {
            width: parent.width
            spacing: 12
            Repeater {
                model: dialog.avatars.rows
                RecentTile {}
            }
        }
        Text {
            visible: !dialog.avatars.loading && dialog.avatars.rows.length === 0 && dialog.avatars.error.length === 0
            text: qsTr("Avatars you upload will show up here.")
            color: Theme.ink2
            font: Theme.type.description
        }
        RowError {
            width: parent.width
            above: 0
            text: dialog.avatars.error
        }
    }

    footer: [
        KitButton {
            variant: "ghost"
            text: qsTr("Cancel")
            onClicked: dialog.close()
        }
    ]

    component RecentTile: Item {
        id: tile
        required property var modelData
        width: 56
        height: 56
        HoverHandler { id: tileHover }
        AbstractButton {
            id: pick
            HoverHandler { cursorShape: Qt.PointingHandCursor }
            anchors.fill: parent
            enabled: !dialog.busy
            Accessible.role: Accessible.Button
            Accessible.name: tile.modelData.current ? qsTr("%1's current avatar").arg(dialog.name) : qsTr("Use this recent avatar")
            onClicked: {
                dialog.recentPicked(tile.modelData.avatarUrl, tile.modelData.url)
                dialog.close()
            }
            scale: down ? Theme.scalePress : 1
            contentItem: Image {
                source: tile.modelData.url
                sourceSize: Qt.size(112, 112)
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
            }
            background: Rectangle {
                radius: Theme.radius
                color: Theme.tint3
            }
        }
        // The current one: the accent ring 2 px out.
        Rectangle {
            anchors.fill: parent
            anchors.margins: -4
            visible: tile.modelData.current || pick.visualFocus
            color: "transparent"
            radius: Theme.radius + 2
            border.width: 2
            border.color: pick.visualFocus ? Theme.focus : Theme.accentInk
        }
        Rectangle {
            anchors.fill: parent
            visible: dialog.avatars.busyId === tile.modelData.id
            color: Qt.rgba(0, 0, 0, 0.6)
            Icon {
                anchors.centerIn: parent
                name: "loader"
                size: Theme.iconSm
                color: "#ffffff"
            }
        }
        IconKey {
            x: parent.width - width + 4
            y: -4
            visible: !tile.modelData.current && (tileHover.hovered || activeFocus)
            enabled: !dialog.busy
            size: 24
            tone: "ghost"
            iconName: "trash"
            tip: qsTr("Delete this recent avatar")
            background: Rectangle {
                radius: 2
                color: Theme.ply3
                border.width: 1
                border.color: Theme.lineStrong
            }
            onClicked: Extras.deleteAvatar(tile.modelData.id)
        }
    }
}
