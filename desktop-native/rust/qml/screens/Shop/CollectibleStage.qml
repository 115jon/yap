pragma ComponentBehavior: Bound
import QtQuick
import Yap.Media
import com.yap.native

// The large live preview of one item on you (CollectiblePreviews.tsx
// CollectibleStage): your avatar (224 px) wearing the decoration, playing;
// a member list with your name on the nameplate between two dim rows,
// playing; or your profile card with the effect (looping, as the shop's
// stage does) or the frame. `detail` is Shop.detail.
Item {
    id: root

    required property var detail

    Loader {
        anchors.centerIn: parent
        sourceComponent: root.detail.kind === "avatar_decoration" ? decoration
            : root.detail.kind === "nameplate" ? nameplate : card
    }

    Component {
        id: decoration
        Item {
            width: 224
            height: 224
            Avatar {
                anchors.fill: parent
                size: 224
                round: true
                initials: Session.meName.length > 0 ? Session.meName : "?"
                source: Session.meAvatar
            }
            Image {
                anchors.centerIn: parent
                width: 224 * 1.2
                height: width
                visible: motion.status !== AnimatedPicture.Ready
                source: root.detail.still
                fillMode: Image.PreserveAspectFit
                asynchronous: true
                sourceSize: Qt.size(240, 240)
            }
            AnimatedPicture {
                id: motion
                anchors.centerIn: parent
                width: 224 * 1.2
                height: width
                source: root.detail.deco
                playing: !Theme.reducedMotion
            }
            Accessible.role: Accessible.Graphic
            Accessible.name: qsTr("Your avatar wearing %1").arg(root.detail.name)
        }
    }

    Component {
        id: nameplate
        Rectangle {
            width: 288
            height: list.implicitHeight + 16
            radius: Theme.radiusSheet * 2
            color: Theme.ply1
            border.width: 1
            border.color: Theme.line
            Accessible.role: Accessible.Graphic
            Accessible.name: qsTr("Your name on %1").arg(root.detail.name)
            Column {
                id: list
                x: 8
                y: 8
                width: parent.width - 16
                spacing: 4
                DimRow { barWidth: 80 }
                Rectangle {
                    width: parent.width
                    height: 48
                    radius: Theme.radiusSheet * 2
                    color: Theme.ply2
                    clip: true
                    NameplateLayer {
                        anchors.fill: parent
                        plate: root.detail.plate ? JSON.stringify(root.detail.plate) : ""
                        playing: true
                    }
                    Avatar {
                        id: face
                        x: 12
                        anchors.verticalCenter: parent.verticalCenter
                        size: 32
                        initials: Session.meName.length > 0 ? Session.meName : "?"
                        source: Session.meAvatar
                        deco: Session.meDeco
                    }
                    Text {
                        anchors.left: face.right
                        anchors.leftMargin: 12
                        anchors.right: parent.right
                        anchors.rightMargin: 12
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: Session.meName
                        elide: Text.ElideRight
                        color: Theme.ink1
                        font: Theme.type.panelTitle
                    }
                }
                DimRow { barWidth: 112 }
            }
        }
    }

    component DimRow: Item {
        property int barWidth: 80
        width: parent ? parent.width : 0
        height: 40
        opacity: 0.5
        Rectangle {
            x: 12
            anchors.verticalCenter: parent.verticalCenter
            width: 28
            height: 28
            radius: Theme.radius
            color: Theme.tint3
        }
        Rectangle {
            x: 52
            anchors.verticalCenter: parent.verticalCenter
            width: parent.barWidth
            height: 8
            radius: 4
            color: Theme.tint3
        }
    }

    Component {
        id: card
        ProfileCard {
            // The profile's own shape (450:880), so effects and frames
            // have the whole card.
            width: 220
            height: Math.round(220 * 880 / 450)
            name: Session.meName
            username: Session.meUsername
            avatar: Session.meAvatar
            initials: Session.meInitials
            presence: Session.meStatus
            customStatus: Session.meCustomStatus
            deco: root.detail.deco
            effect: root.detail.effect ? JSON.stringify(root.detail.effect) : ""
            frame: root.detail.frame ? JSON.stringify(root.detail.frame) : ""
            effectPlayback: "loop"
            Accessible.role: Accessible.Graphic
            Accessible.name: qsTr("Your profile with %1").arg(root.detail.name)
        }
    }
}
