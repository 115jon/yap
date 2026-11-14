import QtQuick
import Yap.Mobile

// A ListView with the platform's scroll feel. Fling speed and friction come
// from the device (Android's ViewConfiguration through Native, in logical
// px). At an edge, Android 12+ stretches the content instead of letting it
// slide: the list keeps its content in place (StopAtBounds) and scales it
// from the far edge by the overshoot, as EdgeEffect's stretch does. iOS
// would keep Qt's own rubber band (DragAndOvershootBounds, FollowBoundsBehavior).
ListView {
    id: list

    readonly property bool stretch: Qt.platform.os === "android"
    // How far the content is pulled past an edge, 0 when inside.
    readonly property real pull: stretch ? verticalOvershoot : 0

    boundsBehavior: Flickable.DragAndOvershootBounds
    boundsMovement: stretch ? Flickable.StopAtBounds : Flickable.FollowBoundsBehavior
    maximumFlickVelocity: Native.maxFlingVelocity
    flickDeceleration: Native.flingDeceleration
    pixelAligned: true
    reuseItems: true
    cacheBuffer: Math.max(0, height)

    // The stretch: up to 5% of the height, eased like EdgeEffect's.
    transform: Scale {
        origin.y: list.pull < 0 ? 0 : list.height
        yScale: 1 + Math.min(0.05, Math.abs(list.pull) / Math.max(1, list.height) * 0.5)
    }
}
