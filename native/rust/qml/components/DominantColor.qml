import QtQuick

// A picture's colourful average, YouTube's ambient colour: the picture drawn
// at 24 px (QQuickPixmap shares the decode cache), each pixel weighted by
// its saturation so white, black and grey backgrounds do not wash it out,
// then saturation lifted and lightness held in a band that glows on black.
// Paints once per source, never shown (opacity 0); `color` is transparent
// until it is known or when there is no picture.
Canvas {
    id: root

    property url source
    property color color: "transparent"
    readonly property bool known: color.a > 0

    width: 24
    height: 24
    opacity: 0
    Accessible.ignored: true

    function measure() {
        color = "transparent"
        if (String(source).length === 0)
            return
        if (isImageLoaded(source))
            requestPaint()
        else
            loadImage(source)
    }
    onSourceChanged: measure()
    Component.onCompleted: measure()
    onImageLoaded: requestPaint()

    onPaint: {
        if (!isImageLoaded(source))
            return
        const ctx = getContext("2d")
        ctx.clearRect(0, 0, width, height)
        ctx.drawImage(source, 0, 0, width, height)
        const d = ctx.getImageData(0, 0, width, height).data
        let r = 0, g = 0, b = 0, sum = 0
        for (let i = 0; i < d.length; i += 4) {
            if (d[i + 3] < 128)
                continue
            const pr = d[i] / 255, pg = d[i + 1] / 255, pb = d[i + 2] / 255
            const max = Math.max(pr, pg, pb), min = Math.min(pr, pg, pb)
            const l = (max + min) / 2
            const s = max === min ? 0 : (max - min) / (1 - Math.abs(2 * l - 1))
            const w = s * (1 - Math.abs(2 * l - 1)) + 0.02
            r += pr * w; g += pg * w; b += pb * w; sum += w
        }
        unloadImage(source)
        if (sum <= 0)
            return
        const avg = Qt.rgba(r / sum, g / sum, b / sum, 1)
        const hue = avg.hslHue < 0 ? 0 : avg.hslHue
        color = Qt.hsla(hue, Math.min(1, avg.hslSaturation * 1.6 + 0.15),
                        Math.max(0.35, Math.min(0.5, avg.hslLightness)), 1)
    }
}
