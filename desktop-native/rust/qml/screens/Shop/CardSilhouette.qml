import QtQuick
import com.yap.native

// A small profile-card silhouette behind effect and frame thumbnails
// (CollectiblePreviews.tsx CardSilhouette): ply-2, a tint-2 banner, the
// avatar key, two name bars and a bio block, all in fractions of its width.
Rectangle {
    id: root

    readonly property real u: width / 100

    radius: 5 * u
    color: Theme.ply2
    clip: true
    Accessible.ignored: true

    Rectangle {
        width: parent.width
        height: 42 * root.u
        color: Theme.tint2
    }
    Rectangle {
        x: 7 * root.u
        y: 30 * root.u
        width: 30 * root.u
        height: width
        radius: Theme.radius
        color: Theme.tint3
        border.width: 2 * root.u
        border.color: Theme.ply2
    }
    Rectangle {
        x: 7 * root.u
        y: 66 * root.u
        width: 46 * root.u
        height: 5 * root.u
        radius: height / 2
        color: Theme.tint3
    }
    Rectangle {
        x: 7 * root.u
        y: 75 * root.u
        width: 30 * root.u
        height: 4 * root.u
        radius: height / 2
        color: Theme.tint2
    }
    Rectangle {
        x: 7 * root.u
        y: 88 * root.u
        width: 86 * root.u
        height: 30 * root.u
        radius: 5 * root.u
        color: Theme.tint1
    }
}
