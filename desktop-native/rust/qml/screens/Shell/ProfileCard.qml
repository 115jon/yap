import QtQuick
import QtQuick.Effects
import QtQuick.Templates as T
import Yap.Media
import com.yap.native
import Yap.Ui

// The profile card (ProfileCard.tsx), at two sizes: `popover` has a 96 px
// banner (the banner picture, else the banner colour, else accent-tint), a
// 72 px avatar half over it with a 4 px ring of the card colour, the name
// at 18/600 and a 16 px inset; `modal` (the full profile, 640 wide) has a
// 176 px banner, a 112 px avatar, the name at 28/600 and a 24 px inset. Then
// "@handle · pronouns" at 13 ink-2, the badges (UserBadges: 24 px, 32 on
// the full profile, wrapping, never cut off) and the status line (LED,
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
//
// `viewable` (the popout): the avatar is a key that opens the full profile
// (`viewRequested`; Tab reaches it, Enter or Space presses it) and a click
// on the banner does too. Hovering either, or focusing the avatar from the
// keyboard, lifts the avatar 4 % and shows a "View profile" chip in the
// banner's top-right corner, 8 px in.
Item {
    id: root

    property string name
    property string username
    property string avatar
    // The avatar display's crop (`{x, y, width, height}` in percent), or
    // null (Avatar.qml).
    property var crop: null
    readonly property var urlCrop: {
        const s = avatar
        const at = s.indexOf("#crop=")
        if (at < 0)
            return null
        const v = s.slice(at + 6).split(",").map(Number)
        return v.length === 4 && v.every(n => isFinite(n)) ? { x: v[0], y: v[1], width: v[2], height: v[3] } : null
    }
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
    // Their badges (the core's JSON), or empty.
    property string badges: ""
    property string effectPlayback: "once"
    property bool animate: true
    // popover | modal
    property string size: "popover"
    readonly property bool modalSize: size === "modal"
    readonly property int bannerHeight: modalSize ? 176 : 96
    readonly property int avatarSize: modalSize ? 112 : 72
    readonly property int pad: modalSize ? 24 : 16
    // The avatar and banner open the full profile (the popout).
    property bool viewable: false
    signal viewRequested()
    readonly property bool viewHint: viewable && (bannerKey.hovered || viewKey.hovered || viewKey.visualFocus)

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
        implicitHeight: 1 + bannerArea.height - (root.avatarSize / 2 + 4) + identity.height
            + (inset.implicitHeight > 0 ? 16 + inset.implicitHeight : 0) + root.pad
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
            height: root.bannerHeight
            color: root.bannerColor.length > 0 ? root.bannerColor
                : root.accentColor.length > 0 ? root.accentColor : Theme.accentTint
            Image {
                anchors.fill: parent
                visible: root.banner.length > 0 && status === Image.Ready && !bannerVideo.shown
                    && !(bannerGif.item && bannerGif.item.status === AnimatedPicture.Ready)
                    && !(bannerAnim.item && bannerAnim.item.status === AnimatedPicture.Ready)
                source: bannerVideo.isVideo(root.banner) ? "" : root.banner
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                clip: true
                sourceSize.width: 680
            }
            // An animated banner plays like the web's <img>, over the still
            // while the card is shown (never under reduced motion); the
            // still above covers the load and anything else. A GIF streams
            // through AnimatedPicture: Qt 6.11's AnimatedImage played it from
            // its network reply, which cannot rewind (it stopped after one
            // pass), and fetched and decoded it afresh at every open. The
            // file stays in the picture cache, so a reopened card shows its
            // first frame at once.
            Loader {
                id: bannerGif
                anchors.fill: parent
                active: root.animatedBanner === "gif"
                sourceComponent: AnimatedPicture {
                    source: root.banner
                    fillMode: AnimatedPicture.PreserveAspectCrop
                    stream: true
                    clip: true
                    // Opacity, not visible (AnimatedPicture loads only while
                    // visible; see bannerAnim).
                    opacity: status === AnimatedPicture.Ready ? 1 : 0
                    playing: root.animate && root.visible && !Theme.reducedMotion
                    Accessible.ignored: true
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
            // The pointer path; the avatar key is the keyboard and
            // screen-reader one.
            T.AbstractButton {
                id: bannerKey
                anchors.fill: parent
                visible: root.viewable
                focusPolicy: Qt.NoFocus
                Accessible.ignored: true
                background: null
                onClicked: root.viewRequested()
                HoverHandler { cursorShape: Qt.PointingHandCursor }
            }
            // "View profile": the card colour and ink, shadow-float.
            Rectangle {
                id: viewChip
                visible: root.viewable && opacity > 0
                anchors.top: parent.top
                anchors.right: parent.right
                anchors.margins: 8
                width: chipRow.implicitWidth + 16
                height: 24
                radius: Theme.radius
                color: root.color
                opacity: root.viewHint ? 1 : 0
                Behavior on opacity { NumberAnimation { duration: Theme.dur2; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
                FloatShadow {}
                Row {
                    id: chipRow
                    anchors.centerIn: parent
                    spacing: 6
                    Icon {
                        anchors.verticalCenter: parent.verticalCenter
                        name: "maximize"
                        size: 12
                        color: root.ink
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: qsTr("View profile")
                        color: root.ink
                        font: Theme.type.captionStrong
                    }
                }
            }
        }

        Column {
            id: identity
            anchors.top: bannerArea.bottom
            // The avatar (inside its 4 px ring) centres on the banner edge.
            anchors.topMargin: -(root.avatarSize / 2 + 4)
            x: root.pad
            width: parent.width - 2 * root.pad
            spacing: 12

            // The avatar key: tint-2 over the card colour, a 4 px card ring.
            Rectangle {
                id: avatarRing
                width: root.avatarSize + 8
                height: root.avatarSize + 8
                x: -4
                radius: Theme.radius
                color: root.color
                // A small lift while the full profile is on offer.
                scale: root.viewHint && !Theme.reducedMotion ? 1.04 : 1
                Behavior on scale { NumberAnimation { duration: Theme.dur2; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
                // Opens the full profile (the popout).
                T.AbstractButton {
                    id: viewKey
                    anchors.fill: parent
                    anchors.margins: 4
                    z: 2
                    visible: root.viewable
                    enabled: root.viewable
                    focusPolicy: Qt.TabFocus
                    Accessible.role: Accessible.Button
                    Accessible.name: qsTr("View %1's full profile").arg(root.name)
                    onClicked: root.viewRequested()
                    Keys.onReturnPressed: root.viewRequested()
                    Keys.onEnterPressed: root.viewRequested()
                    HoverHandler { cursorShape: Qt.PointingHandCursor }
                    // The 2 px focus ring, 2 px out (styles.css :focus-visible).
                    background: Rectangle {
                        anchors.fill: parent
                        anchors.margins: -2
                        visible: viewKey.visualFocus
                        color: "transparent"
                        radius: Theme.radius
                        border.width: 2
                        border.color: Theme.focus
                    }
                }
                Rectangle {
                    anchors.centerIn: parent
                    width: root.avatarSize
                    height: root.avatarSize
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
                    Item {
                        id: face
                        anchors.fill: parent
                        visible: picture.status === Image.Ready && root.deco.length === 0
                        clip: true
                        Image {
                            id: picture
                            // The editor's crop, else the one on the URL
                            // (`#crop=x,y,w,h`, Avatar.qml).
                            readonly property var shown: root.crop || root.urlCrop
                            readonly property var c: shown && shown.width > 0 && shown.height > 0 ? shown : null
                            x: c ? -c.x / c.width * face.width : 0
                            y: c ? -c.y / c.height * face.height : 0
                            width: c ? face.width * 100 / c.width : face.width
                            height: c ? face.height * 100 / c.height : face.height
                            source: root.avatar
                            fillMode: c ? Image.Stretch : Image.PreserveAspectCrop
                            asynchronous: true
                            sourceSize.width: Math.min(2048, Math.ceil(width * 2))
                            sourceSize.height: Math.min(2048, Math.ceil(height * 2))
                        }
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
                                source: face
                                maskEnabled: true
                                maskSource: roundMask
                            }
                        }
                    }
                    AnimatedPicture {
                        visible: root.deco.length > 0
                        anchors.centerIn: parent
                        width: Math.round(root.avatarSize * 1.24)
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
                        // An open profile plays its name (data-decoration-play).
                        playing: root.animate && root.visible
                        width: Math.min(implicitWidth, parent.width - (devices.visible ? devices.implicitWidth + 8 : 0))
                        text: root.name
                        nameStyle: root.nameStyle
                        color: root.ink
                        font: root.modalSize ? Theme.type.profileTitle : Theme.type.profileName
                        // text-ui-2xl's 32 px line (Geist's own is taller).
                        height: root.modalSize ? 32 : implicitHeight
                        ground: root.color
                        Accessible.role: Accessible.Heading
                    }
                    PlatformIcons {
                        id: devices
                        // The full profile has no adornment (UserProfileModal).
                        visible: !root.modalSize
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
                // -ms-0.5: the drawings line up with the text, not their
                // keys' 2 px padding.
                UserBadges {
                    x: -2
                    width: parent.width + 2
                    badges: root.badges
                    size: root.modalSize ? "lg" : "md"
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
            x: root.pad
            width: parent.width - 2 * root.pad
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
