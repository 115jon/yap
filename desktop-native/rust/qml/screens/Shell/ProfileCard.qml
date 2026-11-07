import QtQuick
import QtQuick.Effects
import Yap.Media
import com.yap.native

// The profile card, popover size (ProfileCard.tsx): a 96 px banner (the
// banner picture, else the banner colour, else accent-tint), a 72 px
// avatar half over it with a 4 px ring of the card colour, the name at
// 18/600, "@handle · pronouns" at 13 ink-2 and the status line (LED,
// label, custom status). `statusLine` replaces that line (the account
// card's custom-status field). Children go in the 16 px inset column
// under it (actions, sections). A profile theme (`surfaceColor`,
// `accentColor`, `#RRGGBB`) paints the card and the banner fallback, with
// ink that reads on it; empty follows the app theme.
//
// Collectibles (ProfileSurfaceShell.tsx): the avatar wears its decoration
// (round crop, the APNG at 124 %), the profile effect plays over the card
// (one pass, then its still; `effectPlayback: "loop"` for the shop's
// stage), and a frame draws around it, its back art under the card and its
// front art over it. With a frame the card is inset inside this box by the
// frame's footprint (side, top and bottom as fractions of the box width),
// so the overhang stays inside the box and no scroller clips it.
Item {
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
    property string surfaceColor
    property string accentColor
    property Component statusLine: null
    // Collectibles: the decoration's APNG and the effect and frame JSON.
    property string deco: ""
    property string effect: ""
    property string frame: ""
    // The display-name style (JSON), or empty.
    property string nameStyle: ""
    // Where they are signed in, beside the name (UserPlatformIndicators).
    property string platforms: ""
    property string effectPlayback: "once"
    property bool animate: true

    // `gif`, `anim` (APNG or WebP) or "" (a still or a video), from the
    // banner's extension or the type the core put in its fragment.
    readonly property string animatedBanner: /\.gif($|[?#])|#image\/gif/i.test(banner) ? "gif"
        : /\.(apng|png|webp)($|[?#])|#image\/(apng|png|webp)/i.test(banner) ? "anim" : ""
    readonly property var frameInfo: frame.length > 0 ? JSON.parse(frame) : null
    readonly property real inSide: frameInfo ? frameInfo.side * width : 0
    readonly property real inTop: frameInfo ? frameInfo.top * width : 0
    readonly property real inBottom: frameInfo ? frameInfo.bottom * width : 0
    // The card's colour (the profile theme's, else ply-3).
    property alias color: surface.color

    readonly property bool themed: surfaceColor.length > 0
    readonly property bool lightSurface: themed && (0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b) > 0.5
    readonly property color ink: !themed ? Theme.ink1 : lightSurface ? "#0b0b0d" : "#f2f2f3"
    readonly property color subInk: !themed ? Theme.ink2 : lightSurface ? "#4a4a52" : "#b4b4ba"
    default property alias content: inset.data

    // A popover (not the settings preview): shadow-2.
    property bool floating: false

    implicitWidth: 340
    implicitHeight: inTop + surface.implicitHeight + inBottom

    ProfileFrameLayer {
        visible: root.frameInfo !== null
        width: root.width
        height: root.height
        frame: root.frame
        order: "back"
    }

    Rectangle {
        id: surface
        x: root.inSide
        y: root.inTop
        width: root.width - 2 * root.inSide
        height: root.height - root.inTop - root.inBottom
        implicitHeight: 1 + bannerArea.height - 40 + identity.height
            + (inset.implicitHeight > 0 ? 16 + inset.implicitHeight : 0) + 16
        radius: Theme.radius
        color: root.themed ? root.surfaceColor : Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        FloatShadow { visible: root.floating }

        Rectangle {
            id: bannerArea
            x: 1
            y: 1
            width: parent.width - 2
            height: 96
            color: root.bannerColor.length > 0 ? root.bannerColor
                : root.accentColor.length > 0 ? root.accentColor : Theme.accentTint
            Image {
                anchors.fill: parent
                visible: root.banner.length > 0 && status === Image.Ready && !bannerVideo.shown
                    && !(bannerGif.item && bannerGif.item.status === AnimatedImage.Ready)
                    && !(bannerAnim.item && bannerAnim.item.status === AnimatedPicture.Ready)
                source: bannerVideo.isVideo(root.banner) ? "" : root.banner
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                clip: true
                sourceSize.width: 680
            }
            // An animated banner plays like the web's <img>: a GIF through
            // AnimatedImage, an APNG or animated WebP through AnimatedPicture,
            // over the still while the card is shown (never under reduced
            // motion). The still above covers the load and anything else.
            Loader {
                id: bannerGif
                anchors.fill: parent
                active: root.animatedBanner === "gif"
                sourceComponent: AnimatedImage {
                    source: root.banner
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                    clip: true
                    visible: status === AnimatedImage.Ready
                    playing: root.animate && root.visible && !Theme.reducedMotion
                }
            }
            Loader {
                id: bannerAnim
                anchors.fill: parent
                active: root.animatedBanner === "anim"
                sourceComponent: AnimatedPicture {
                    source: root.banner
                    fillMode: AnimatedPicture.PreserveAspectCrop
                    clip: true
                    // Opacity, not visible: AnimatedPicture loads only while
                    // visible, so `visible: status === Ready` never loaded it
                    // (the banner stayed on its still).
                    opacity: status === AnimatedPicture.Ready ? 1 : 0
                    playing: root.animate && root.visible && !Theme.reducedMotion
                    Accessible.ignored: true
                }
            }
            // A video banner plays while the card is open (ProfileCard.tsx's
            // ProfileAssetLayer).
            ProfileVideoLayer {
                id: bannerVideo
                anchors.fill: parent
                source: isVideo(root.banner) ? root.banner : ""
                playing: root.visible
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
                        textFormat: Text.PlainText
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
                        visible: status === Image.Ready && root.deco.length === 0
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                        sourceSize.width: 144
                        sourceSize.height: 144
                    }
                    // Decorated: the face crops round under the ring art.
                    Loader {
                        anchors.fill: parent
                        active: root.deco.length > 0 && picture.status === Image.Ready
                        sourceComponent: Item {
                            Rectangle {
                                id: roundMask
                                anchors.fill: parent
                                radius: width / 2
                                visible: false
                                layer.enabled: true
                            }
                            MultiEffect {
                                anchors.fill: parent
                                source: picture
                                maskEnabled: true
                                maskSource: roundMask
                            }
                        }
                    }
                    AnimatedPicture {
                        visible: root.deco.length > 0
                        anchors.centerIn: parent
                        width: Math.round(72 * 1.24)
                        height: width
                        source: root.deco
                        playing: root.animate && !Theme.reducedMotion
                    }
                }
            }

            Column {
                width: parent.width
                spacing: 4
                Row {
                    width: parent.width
                    spacing: 8
                    DisplayName {
                        id: nameLine
                        width: Math.min(implicitWidth, parent.width - (devices.visible ? devices.implicitWidth + 8 : 0))
                        text: root.name
                        nameStyle: root.nameStyle
                        color: root.ink
                        font: Theme.type.profileName
                        ground: root.color
                        Accessible.role: Accessible.Heading
                    }
                    PlatformIcons {
                        id: devices
                        anchors.verticalCenter: nameLine.verticalCenter
                        platforms: root.platforms
                        presence: root.presence
                        size: Theme.iconSm
                        ink: root.subInk
                        offlineInk: root.themed ? root.subInk : Theme.ink3
                    }
                }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: root.pronouns.length > 0 ? "@" + root.username + "  ·  " + root.pronouns : "@" + root.username
                    visible: root.username.length > 0
                    wrapMode: Text.WrapAnywhere
                    color: root.subInk
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
                        textFormat: Text.PlainText
                        text: Theme.presenceWord(root.presence)
                        color: root.subInk
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
                        textFormat: Text.PlainText
                        text: root.customStatus
                        elide: Text.ElideRight
                        color: root.ink
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

        ProfileEffectLayer {
            anchors.fill: parent
            anchors.margins: 1
            visible: root.effect.length > 0
            effect: root.effect
            playback: root.effectPlayback
            playing: root.animate && root.visible
        }
    }

    ProfileFrameLayer {
        visible: root.frameInfo !== null
        width: root.width
        height: root.height
        frame: root.frame
        order: "front"
    }
}
