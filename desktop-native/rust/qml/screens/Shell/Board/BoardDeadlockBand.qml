import QtQuick
import com.yap.native
import Yap.Ui

// The hero art band (Deadlock.tsx Band / ArtTag / ArtClock): the hero's
// colour as a 140° gradient into black, a faint 6 px grille, the hero card
// standing on the right and overflowing the top. Optional: a dark tag at
// the top left (with the talk LED in a match) and a big clock bottom left.
// Game art keeps its own colours, so the text on it is fixed light ink.
// Painted once per size and colour; nothing moves at rest.
Item {
    id: band

    required property var dl
    required property var hero
    property string tag: ""
    property bool tagLed: false
    property string clockLabel: ""
    property string clock: ""

    clip: true
    Accessible.ignored: true

    Canvas {
        id: ground
        anchors.fill: parent
        readonly property color hc: band.hero?.color ?? "#5A5A5A"
        onHcChanged: requestPaint()
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onPaint: {
            const g = getContext("2d")
            g.reset()
            const w = width, h = height
            // 140deg: from the top left towards the bottom right.
            const a = 140 * Math.PI / 180
            const dx = Math.sin(a), dy = -Math.cos(a)
            const half = (Math.abs(w * dx) + Math.abs(h * dy)) / 2
            const grad = g.createLinearGradient(w / 2 - dx * half, h / 2 - dy * half, w / 2 + dx * half, h / 2 + dy * half)
            grad.addColorStop(0, Qt.rgba(hc.r * 0.78, hc.g * 0.78, hc.b * 0.78, 1))
            grad.addColorStop(1, Qt.rgba(hc.r * 0.30, hc.g * 0.30, hc.b * 0.30, 1))
            g.fillStyle = grad
            g.fillRect(0, 0, w, h)
            g.fillStyle = Qt.rgba(1, 1, 1, 0.04)
            for (let x = 0; x < w; x += 6)
                g.fillRect(x, 0, 1, h)
        }
    }
    Image {
        id: card
        visible: status === Image.Ready
        y: -8
        height: Math.round(band.height * 1.45)
        width: implicitHeight > 0 ? Math.round(implicitWidth * height / implicitHeight) : 0
        x: band.width - width - 6
        source: band.dl.heroArt(band.hero, "card")
        sourceSize.height: height * 2
        fillMode: Image.PreserveAspectFit
        asynchronous: true
        cache: true
        smooth: true
        mipmap: true
    }
    Rectangle {
        visible: band.tag.length > 0
        x: 10
        y: 10
        height: 22
        width: tagRow.implicitWidth + 14
        radius: Theme.radius
        color: Qt.rgba(0, 0, 0, 0.6)
        Row {
            id: tagRow
            x: 7
            anchors.verticalCenter: parent.verticalCenter
            spacing: 6
            Led {
                visible: band.tagLed
                anchors.verticalCenter: parent.verticalCenter
                lit: true
                color: Theme.talk
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: band.tag
                color: "#f2f2f3"
                font: Theme.type.silk
            }
        }
    }
    Column {
        visible: band.clock.length > 0
        x: 12
        y: band.height - height - 10
        spacing: 4
        Text {
            textFormat: Text.PlainText
            text: band.clockLabel
            color: "#d8d8dc"
            font: Theme.type.silk
        }
        Text {
            textFormat: Text.PlainText
            text: band.clock
            color: "#f2f2f3"
            style: Text.Raised
            styleColor: Qt.rgba(0, 0, 0, 0.35)
            font: Qt.font({ family: "Yap Martian Num SemiBold", pixelSize: 30, weight: Font.DemiBold, letterSpacing: -0.6 })
        }
    }
}
