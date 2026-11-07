pragma ComponentBehavior: Bound
import QtQuick
import Yap.Media
import com.yap.native

// A profile effect over a profile surface (ProfileCollectiblesLayer.tsx,
// profile-effect-playback.ts). The effect's 450 x 880 stage covers the
// surface's width from the top; each layer (an APNG) shows from its
// `start` for its `duration`, in its stage rectangle, above the ones under
// it. `once` (every profile surface) plays one pass when the surface opens
// and then rests on the effect's still with a short fade; `loop` (the
// shop's stage) keeps the authored loop. Under reduced motion only the
// still shows. Nothing ticks once the pass rests.
Item {
    id: root

    // The store's effect JSON: `{layers: [{src, loop, start, duration, x,
    // y, w, h, z}], still}`, or empty.
    property string effect: ""
    property string playback: "once"
    property bool playing: true

    readonly property var info: effect.length > 0 ? JSON.parse(effect) : null
    readonly property bool canAnimate: playing && !Theme.reducedMotion && info !== null && info.layers.length > 0
    // An untimed layer counts this long in a pass.
    readonly property int untimedCap: 6000
    readonly property int deadline: info === null ? 0
        : info.layers.reduce((end, l) => Math.max(end, l.start + (l.duration > 0 ? l.duration : untimedCap)), 0)

    property int elapsed: 0
    property double startedAt: 0
    property bool resting: false

    function restartPass() {
        root.resting = false
        root.elapsed = 0
        root.startedAt = Date.now()
    }
    onEffectChanged: restartPass()
    onCanAnimateChanged: if (canAnimate) restartPass()
    Component.onCompleted: restartPass()

    clip: true
    Accessible.ignored: true

    Timer {
        interval: 50
        repeat: true
        running: root.canAnimate && !root.resting && root.visible
            && Qt.application.state === Qt.ApplicationActive
        onTriggered: {
            root.elapsed = Date.now() - root.startedAt
            if (root.playback === "once" && root.elapsed >= root.deadline)
                root.resting = true
        }
    }

    Item {
        id: stage
        width: root.width
        height: Math.round(root.width * 880 / 450)
        visible: root.canAnimate && !root.resting

        Repeater {
            model: root.canAnimate ? root.info.layers : []
            AnimatedPicture {
                id: layer
                required property var modelData
                readonly property bool timed: modelData.duration > 0
                readonly property bool looping: root.playback === "loop" && modelData.loop
                readonly property bool active: root.elapsed >= modelData.start
                    && (looping || !timed || root.elapsed < modelData.start + modelData.duration)
                x: modelData.x * stage.width
                y: modelData.y * stage.height
                width: modelData.w * stage.width
                height: modelData.h * stage.height
                z: modelData.z
                source: modelData.src
                fillMode: AnimatedPicture.Stretch
                visible: active
                playing: active
                loops: looping ? 0 : 1
                onActiveChanged: if (active) restart()
            }
        }
    }

    // The still: under reduced motion, and where a pass comes to rest.
    Image {
        width: stage.width
        height: stage.height
        visible: root.info !== null && root.info.still.length > 0 && (!root.canAnimate || root.resting)
        source: visible ? root.info.still : ""
        fillMode: Image.Stretch
        asynchronous: true
        sourceSize.width: Math.ceil(width * 2)
        opacity: visible ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Theme.dur3 } }
    }
}
