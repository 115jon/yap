pragma ComponentBehavior: Bound
import QtQuick
import QtMultimedia
import Yap.Chrome
import Yap.Media
import com.yap.native
import Yap.Ui

// Floating sticker reactions over the call (StickerReactionsOverlay.tsx):
// each `reaction.sticker` that lands plays once in its display mode:
// single (one rises, tilted), burst (eight fly out from the bottom
// centre), rain (twelve fall from the top) or bounce (one lands a quarter
// down, holds, goes). Pictures animate; mp4 stickers play muted.
Item {
    id: root

    // Plays new stickers only while the call shows.
    property bool active: true
    readonly property var stickers: JSON.parse(Calls.view).stickers ?? []
    // The newest sticker already played (older ones never replay).
    property int played: 0

    clip: true
    Accessible.ignored: true

    // Opening the call view does not replay what came before.
    Component.onCompleted: {
        const list = root.stickers
        root.played = list.length > 0 ? list[list.length - 1].seq : 0
    }
    onStickersChanged: {
        const list = root.stickers
        for (const s of list) {
            if (s.seq <= root.played)
                continue
            root.played = s.seq
            if (root.active && root.width > 0)
                root.spawn(s)
        }
    }

    function spawn(s) {
        const video = s.contentType === "video/mp4"
        const w = root.width
        const h = root.height
        const rnd = (a, b) => a + Math.random() * (b - a)
        if (s.displayMode === "burst") {
            for (let i = 0; i < 8; i++) {
                const angle = (i * 45 + rnd(-7.5, 7.5)) * Math.PI / 180
                const dist = rnd(80, 160)
                particle.createObject(root, {
                    url: s.url, video: video, size: 48,
                    fromX: w / 2 - 24, fromY: h - 64,
                    toX: w / 2 - 24 + Math.cos(angle) * dist, toY: h - 64 + Math.sin(angle) * dist,
                    turn: rnd(-90, 90), delay: rnd(0, 150), duration: 1400, mode: "burst"
                })
            }
        } else if (s.displayMode === "rain") {
            for (let i = 0; i < 12; i++) {
                const x = w * rnd(0.05, 0.95) - 24
                particle.createObject(root, {
                    url: s.url, video: video, size: 48,
                    fromX: x, fromY: -48, toX: x, toY: h,
                    turn: rnd(0, 360), delay: rnd(0, 1200), duration: 2200, mode: "rain"
                })
            }
        } else if (s.displayMode === "bounce") {
            const x = w * rnd(0.2, 0.8) - 32
            particle.createObject(root, {
                url: s.url, video: video, size: 64,
                fromX: x, fromY: h * 0.25 - 48, toX: x, toY: h * 0.25,
                turn: rnd(-8, 8), delay: 0, duration: 2400, mode: "bounce"
            })
        } else {
            const x = w * rnd(0.2, 0.8) - 32
            particle.createObject(root, {
                url: s.url, video: video, size: 64,
                fromX: x, fromY: h - 64, toX: x, toY: Math.max(0, h * 0.15),
                turn: rnd(-20, 20), delay: 0, duration: 3000, mode: "single"
            })
        }
    }

    Component {
        id: particle
        Item {
            id: p
            property url url
            property bool video: false
            property int size: 64
            property real fromX
            property real fromY
            property real toX
            property real toY
            property real turn
            property int delay
            property int duration
            property string mode

            width: size
            height: size
            x: fromX
            y: fromY
            opacity: 0
            rotation: mode === "single" || mode === "bounce" ? turn : 0

            // Streamed so a GIF sticker loops for its whole flight
            // (AnimatedImage played it from its reply, which cannot rewind).
            AnimatedPicture {
                anchors.fill: parent
                visible: !p.video
                source: p.video ? "" : p.url
                fillMode: AnimatedPicture.PreserveAspectFit
                stream: true
                playing: true
            }
            Loader {
                anchors.fill: parent
                active: p.video && Compat.mediaPlayback
                sourceComponent: Video {
                    source: p.url
                    muted: true
                    loops: MediaPlayer.Infinite
                    fillMode: VideoOutput.PreserveAspectFit
                    Component.onCompleted: play()
                }
            }

            SequentialAnimation {
                running: true
                PauseAnimation { duration: p.delay }
                ParallelAnimation {
                    XAnimator {
                        target: p
                        from: p.fromX
                        to: p.toX
                        duration: p.mode === "bounce" ? 260 : p.duration
                        easing.type: p.mode === "rain" ? Easing.InQuad : Easing.OutCubic
                    }
                    YAnimator {
                        target: p
                        from: p.fromY
                        to: p.toY
                        duration: p.mode === "bounce" ? 260 : p.duration
                        easing.type: p.mode === "rain" ? Easing.InQuad : Easing.OutCubic
                    }
                    RotationAnimator {
                        target: p
                        from: p.mode === "burst" || p.mode === "rain" ? 0 : p.turn
                        to: p.turn
                        duration: p.duration
                    }
                    SequentialAnimation {
                        OpacityAnimator { target: p; from: 0; to: 1; duration: 150 }
                        PauseAnimation { duration: Math.max(0, p.duration - 600) }
                        OpacityAnimator { target: p; from: 1; to: 0; duration: 450 }
                    }
                }
                ScriptAction { script: p.destroy() }
            }
        }
    }
}
