import QtQuick
import com.yap.native

// Your profile card as the shop and the pickers preview it
// (CollectiblePreviews.tsx ProfileCardPreview): the profile's own shape
// (450:880), a tint-2 banner, your avatar key, your name and a bio block,
// every size a fraction of the card width, so it reads the same at 84 px or
// 280. The effect plays over it (looping, as a stage does) and the frame
// is drawn around it. `effect` and `frame` are the store's JSON strings.
Item {
    id: root

    property string effect: ""
    property string frame: ""
    property string deco: ""
    property bool showUsername: true
    property bool playEffect: true

    readonly property var frameInfo: frame.length > 0 ? JSON.parse(frame) : null
    // The card inside the frame's footprint, in fractions of the box width.
    readonly property real inSide: frameInfo ? frameInfo.side * width : 0
    readonly property real inTop: frameInfo ? frameInfo.top * width : 0
    readonly property real inBottom: frameInfo ? frameInfo.bottom * width : 0
    readonly property real u: card.width / 100
    readonly property bool roundAvatar: deco.length > 0

    implicitWidth: 220
    implicitHeight: Math.round(width * 880 / 450)

    ProfileFrameLayer {
        anchors.fill: parent
        frame: root.frame
        order: "back"
    }

    Rectangle {
        id: card
        x: root.inSide
        y: root.inTop
        width: root.width - 2 * root.inSide
        height: root.height - root.inTop - root.inBottom
        radius: 7 * root.u
        color: Theme.ply2
        border.width: 1
        border.color: Theme.line
        clip: true

        Rectangle {
            width: parent.width
            height: 42 * root.u
            color: Theme.tint2
        }
        Item {
            x: 7 * root.u
            y: 26 * root.u
            width: 30 * root.u
            height: width
            // The halo: a decoration's ring is drawn for a circle.
            Rectangle {
                anchors.fill: parent
                anchors.margins: -0.04 * parent.width
                radius: root.roundAvatar ? width / 2 : Theme.radius
                color: Theme.ply2
            }
            Avatar {
                anchors.fill: parent
                size: parent.width
                round: root.roundAvatar
                initials: Session.meName.length > 0 ? Session.meName : "?"
                source: Session.meAvatar
                deco: root.deco
            }
        }
        Column {
            x: 7 * root.u
            y: 60 * root.u
            width: parent.width - 14 * root.u
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: Session.meName
                elide: Text.ElideRight
                color: Theme.ink1
                font.family: Theme.type.panelTitle.family
                font.weight: Font.DemiBold
                font.pixelSize: Math.max(1, 7.5 * root.u)
                lineHeight: 1.25
            }
            Text {
                visible: root.showUsername && Session.meUsername.length > 0
                width: parent.width
                textFormat: Text.PlainText
                text: "@" + Session.meUsername
                elide: Text.ElideRight
                color: Theme.ink3
                font.family: Theme.type.caption.family
                font.pixelSize: Math.max(1, 5 * root.u)
                lineHeight: 1.25
            }
            Item { width: 1; height: 6 * root.u }
            Rectangle {
                width: parent.width
                height: bars.implicitHeight + 8 * root.u
                radius: 4 * root.u
                color: Theme.tint1
                Column {
                    id: bars
                    x: 4 * root.u
                    y: 4 * root.u
                    width: parent.width - 8 * root.u
                    spacing: 2.5 * root.u
                    Rectangle { width: parent.width * 0.38; height: 2.4 * root.u; radius: height / 2; color: Theme.tint3 }
                    Rectangle { width: parent.width * 0.82; height: 2 * root.u; radius: height / 2; color: Theme.tint2 }
                    Rectangle { width: parent.width * 0.64; height: 2 * root.u; radius: height / 2; color: Theme.tint2 }
                }
            }
        }
        ProfileEffectLayer {
            anchors.fill: parent
            effect: root.effect
            playback: "loop"
            playing: root.playEffect && root.visible
        }
    }

    ProfileFrameLayer {
        anchors.fill: parent
        frame: root.frame
        order: "front"
    }
}
