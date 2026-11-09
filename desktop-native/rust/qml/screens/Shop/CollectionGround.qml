import QtQuick
import com.yap.native

// A collection's own colours behind its art (the catalog's CSS
// `linear-gradient`, as `{vertical, stops: [[at, colour], ...]}`; up to
// three stops), else tint-1.
Rectangle {
    id: root

    property var gradientInfo: null

    readonly property var stops: gradientInfo && gradientInfo.stops ? gradientInfo.stops : []
    function stop(i) {
        return stops.length === 0 ? [i / 2, Theme.tint1] : stops[Math.min(i, stops.length - 1)]
    }

    color: Theme.tint1
    gradient: fill

    Gradient {
        id: fill
        orientation: root.gradientInfo && root.gradientInfo.vertical === false ? Gradient.Horizontal : Gradient.Vertical
        GradientStop { position: root.stop(0)[0]; color: root.stop(0)[1] }
        GradientStop { position: root.stops.length > 2 ? root.stop(1)[0] : root.stop(1)[0] * 0.999; color: root.stop(1)[1] }
        GradientStop { position: root.stop(2)[0]; color: root.stop(2)[1] }
    }
}
