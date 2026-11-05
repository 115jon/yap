import QtQuick
import com.yap.native

// The profile card, popover size (ProfileCard.tsx): a 96 px banner (the
// banner picture, else the banner colour, else accent-tint), a 72 px
// avatar half over it with a 4 px ring of the card colour, the name at
// 18/600, "@handle · pronouns" at 13 ink-2 and the status line (LED,
// label, custom status). `statusLine` replaces that line (the account
// card's custom-status field). Children go in the 16 px inset column
// under it (actions, sections).
Rectangle {
    id: root

    property string name
    property string username
    property string avatar
    property string initials
    property string pronouns
    property string presence
    property string customStatus
    property string banner
    property string bannerColor
    property Component statusLine: null
    default property alias content: inset.data

    implicitWidth: 340
    implicitHeight: 1 + bannerArea.height - 40 + identity.height
        + (inset.implicitHeight > 0 ? 16 + inset.implicitHeight : 0) + 16
    radius: Theme.radius
    color: Theme.ply3
    border.width: 1
    border.color: Theme.lineStrong

    Rectangle {
        id: bannerArea
        x: 1
        y: 1
        width: parent.width - 2
        height: 96
        color: root.bannerColor.length > 0 ? root.bannerColor : Theme.accentTint
        Image {
            anchors.fill: parent
            visible: root.banner.length > 0 && status === Image.Ready
            source: root.banner
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            clip: true
            sourceSize.width: 680
        }
    }

    Column {
        id: identity
        anchors.top: bannerArea.bottom
        // The 72 px avatar (inside its 4 px ring) centres on the banner edge.
        anchors.topMargin: -40
        x: 16
        width: parent.width - 32
        spacing: 12

        // The avatar key: tint-2 over the card colour, a 4 px card ring.
        Rectangle {
            width: 72 + 8
            height: 72 + 8
            x: -4
            radius: Theme.radius
            color: root.color
            Rectangle {
                anchors.centerIn: parent
                width: 72
                height: 72
                radius: Theme.radius
                color: Qt.tint(root.color, Theme.tint2)
                Text {
                    anchors.centerIn: parent
                    visible: picture.status !== Image.Ready
                    text: root.initials.charAt(0)
                    color: Theme.ink1
                    font.family: "Yap Bricolage Key ExtraBold"
                    font.pixelSize: 24
                    font.weight: Font.ExtraBold
                }
                Image {
                    id: picture
                    anchors.fill: parent
                    source: root.avatar
                    visible: status === Image.Ready
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                    sourceSize.width: 144
                    sourceSize.height: 144
                }
            }
        }

        Column {
            width: parent.width
            spacing: 4
            Text {
                width: parent.width
                text: root.name
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.profileName
                Accessible.role: Accessible.Heading
            }
            Text {
                width: parent.width
                text: root.pronouns.length > 0 ? "@" + root.username + "  ·  " + root.pronouns : "@" + root.username
                visible: root.username.length > 0
                wrapMode: Text.WrapAnywhere
                color: Theme.ink2
                font: Theme.type.description
            }
            Loader {
                width: parent.width
                active: root.statusLine !== null
                visible: active
                sourceComponent: root.statusLine
            }
            // Presence, then the custom status after a dot.
            Row {
                visible: root.statusLine === null && root.presence.length > 0
                width: parent.width
                height: 18
                spacing: 8
                Led {
                    anchors.verticalCenter: parent.verticalCenter
                    size: 8
                    lit: root.presence !== "offline"
                    color: Theme.presenceLed(root.presence)
                }
                Text {
                    id: word
                    anchors.verticalCenter: parent.verticalCenter
                    text: Theme.presenceWord(root.presence)
                    color: Theme.ink2
                    font: Theme.type.description
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.customStatus.length > 0
                    text: "·"
                    color: Theme.ink3
                    font: Theme.type.description
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.customStatus.length > 0
                    width: Math.min(implicitWidth, parent.width - 16 - word.width - 24)
                    text: root.customStatus
                    elide: Text.ElideRight
                    color: Theme.ink1
                    font: Theme.type.description
                }
            }
        }
    }

    Column {
        id: inset
        anchors.top: identity.bottom
        anchors.topMargin: 16
        x: 16
        width: parent.width - 32
        spacing: 16
    }
}
