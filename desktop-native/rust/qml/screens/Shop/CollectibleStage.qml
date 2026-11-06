pragma ComponentBehavior: Bound
import QtQuick
import Yap.Media
import com.yap.native

// The large live preview of one item on you (CollectiblePreviews.tsx
// CollectibleStage): your avatar wearing the decoration, playing (square
// and bare when there is none); a member list with your name on the
// nameplate between two dim rows, playing; or your profile card
// (CardPreview) with the effect looping or the frame. `detail` is
// Shop.detail or the picker's `preview`. The decoration's box and inset
// and the card's height are the caller's (the shop's detail: 224 / 24 and
// 400; the picker: 208 / 24 and up to 520).
Item {
    id: root

    required property var detail
    property int decorationBox: 224
    property int decorationInset: 24
    property int cardHeight: 400

    Loader {
        anchors.centerIn: parent
        sourceComponent: root.detail.kind === "avatar_decoration" ? decoration
            : root.detail.kind === "nameplate" ? nameplate : card
    }

    Component {
        id: decoration
        Item {
            readonly property int face: root.decorationBox - 2 * root.decorationInset
            readonly property bool worn: root.detail.deco.length > 0
            width: face
            height: face
            Avatar {
                anchors.fill: parent
                size: parent.face
                round: parent.worn
                initials: Session.meName.length > 0 ? Session.meName : "?"
                source: Session.meAvatar
            }
            Image {
                anchors.centerIn: parent
                width: parent.face * 1.2
                height: width
                visible: parent.worn && motion.status !== AnimatedPicture.Ready
                source: parent.worn ? root.detail.still : ""
                fillMode: Image.PreserveAspectFit
                asynchronous: true
                sourceSize: Qt.size(240, 240)
            }
            AnimatedPicture {
                id: motion
                anchors.centerIn: parent
                width: parent.face * 1.2
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
        CardPreview {
            height: root.cardHeight
            width: Math.round(height * 450 / 880)
            deco: root.detail.deco
            effect: root.detail.effect ? JSON.stringify(root.detail.effect) : ""
            frame: root.detail.frame ? JSON.stringify(root.detail.frame) : ""
            Accessible.role: Accessible.Graphic
            Accessible.name: qsTr("Your profile with %1").arg(root.detail.name)
        }
    }
}
