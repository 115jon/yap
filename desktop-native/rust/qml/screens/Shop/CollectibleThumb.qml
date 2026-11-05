import QtQuick
import Yap.Media
import com.yap.native

// An item on a shop tile (ShopItemCard.tsx ItemStage, CollectiblePreviews):
//   avatar_decoration  your avatar (round) at 44 % of the tile, the still
//                      ring over it at 120 %; the APNG while `animate`.
//   profile_effect     the effect's still over a card silhouette, 86 % high.
//   profile_frame      the frame around a card silhouette, 70 % high.
//   nameplate          your avatar and name on the plate (88 % wide); the
//                      webm plays while `animate`.
Item {
    id: root

    required property string kind
    required property string still
    property string motion: ""
    property var frame: null
    property bool animate: false

    Loader {
        anchors.fill: parent
        sourceComponent: root.kind === "avatar_decoration" ? decoration
            : root.kind === "profile_effect" ? effect
            : root.kind === "profile_frame" ? frameThumb : nameplate
    }

    Component {
        id: decoration
        Item {
            Item {
                id: face
                anchors.centerIn: parent
                width: Math.min(144, parent.width * 0.44)
                height: width
                Avatar {
                    anchors.fill: parent
                    size: parent.width
                    round: true
                    initials: Session.meName.length > 0 ? Session.meName : "?"
                    source: Session.meAvatar
                }
                Image {
                    anchors.centerIn: parent
                    width: parent.width * 1.2
                    height: width
                    visible: !motionPicture.visible
                    source: root.still
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                    sourceSize: Qt.size(160, 160)
                }
                AnimatedPicture {
                    id: motionPicture
                    anchors.centerIn: parent
                    width: parent.width * 1.2
                    height: width
                    // Only the hovered tile loads its APNG.
                    source: root.animate && root.motion.length > 0 ? root.motion : ""
                    visible: root.animate && status === AnimatedPicture.Ready
                    playing: root.animate && !Theme.reducedMotion
                }
            }
        }
    }

    Component {
        id: effect
        Item {
            CardSilhouette {
                anchors.centerIn: parent
                height: parent.height * 0.86
                width: height * 450 / 880
                Image {
                    anchors.fill: parent
                    source: root.still
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                    sourceSize.width: 150
                }
            }
        }
    }

    Component {
        id: frameThumb
        Item {
            Item {
                id: box
                readonly property var info: root.frame
                anchors.centerIn: parent
                height: parent.height * 0.7
                width: height * 450 / 880
                CardSilhouette {
                    x: box.info ? box.info.side * box.width : 0
                    y: box.info ? box.info.top * box.width : 0
                    width: box.width - 2 * x
                    height: box.height - y - (box.info ? box.info.bottom * box.width : 0)
                    z: 1
                }
                ProfileFrameLayer {
                    anchors.fill: parent
                    frame: box.info ? JSON.stringify(box.info) : ""
                    order: "back"
                    z: 0
                }
                ProfileFrameLayer {
                    anchors.fill: parent
                    frame: box.info ? JSON.stringify(box.info) : ""
                    order: "front"
                    z: 2
                }
            }
        }
    }

    Component {
        id: nameplate
        Item {
            Rectangle {
                anchors.centerIn: parent
                width: parent.width * 0.88
                height: 48
                radius: Theme.radiusSheet * 2
                color: Theme.ply2
                clip: true
                NameplateLayer {
                    anchors.fill: parent
                    plate: JSON.stringify({ still: root.still, motion: root.motion, border: "" })
                    playing: root.animate
                }
                Avatar {
                    id: plateFace
                    x: 12
                    anchors.verticalCenter: parent.verticalCenter
                    size: 32
                    initials: Session.meName.length > 0 ? Session.meName : "?"
                    source: Session.meAvatar
                }
                Text {
                    anchors.left: plateFace.right
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
        }
    }
}
