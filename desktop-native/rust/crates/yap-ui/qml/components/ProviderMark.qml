import QtQuick
import QtQuick.Effects
import Yap.Ui

// The GIF provider's mark (GifPickerModal.tsx ProviderMark), for the
// picker's "Powered by" line in both apps: KLIPY's wordmark through a mask
// in `color` (ink-2), so it holds in every theme; Tenor is its name in 600.
// The wordmark is the app's resource `qrc:/svg/klipy-text-light.svg`
// (desktop-native/assets/svg).
Item {
    id: mark

    property string provider: "klipy"
    // The wordmark's height (and Tenor's type size).
    property real size: 12
    property color color: Theme.ink2

    implicitWidth: provider === "tenor" ? tenor.implicitWidth : Math.round(size * 375.1 / 102.5)
    implicitHeight: provider === "tenor" ? tenor.implicitHeight : size
    Accessible.role: Accessible.Graphic
    Accessible.name: provider === "tenor" ? "Tenor" : "KLIPY"

    Item {
        visible: mark.provider !== "tenor"
        anchors.fill: parent
        Image {
            id: klipyMask
            anchors.fill: parent
            source: "qrc:/svg/klipy-text-light.svg"
            // Rasterized at the screen's pixels, not a fixed 2x scaled down.
            sourceSize: Qt.size(Math.ceil(width * Screen.devicePixelRatio), Math.ceil(height * Screen.devicePixelRatio))
            smooth: true
            visible: false
            layer.enabled: true
            layer.smooth: true
        }
        Rectangle {
            id: klipyInk
            anchors.fill: parent
            color: mark.color
            visible: false
            layer.enabled: true
        }
        MultiEffect {
            anchors.fill: parent
            source: klipyInk
            maskEnabled: true
            maskSource: klipyMask
            // The wordmark's alpha as coverage: MultiEffect takes
            // smoothstep(min (1 + s) - s, min (1 + s)), here 0..0.5, so the
            // ground stays clear, edges ramp and the 12 px strokes (alpha
            // ~0.7 at most) reach full ink. The default hard threshold at 0
            // turned every edge pixel solid (the jagged mark).
            maskThresholdMin: 0.3334
            maskSpreadAtMin: 0.5
        }
    }
    Text {
        id: tenor
        visible: mark.provider === "tenor"
        anchors.verticalCenter: parent.verticalCenter
        textFormat: Text.PlainText
        text: "Tenor"
        color: mark.color
        font.family: Theme.type.labelStrong.family
        font.weight: Theme.type.labelStrong.weight
        font.pixelSize: mark.size
    }
}
