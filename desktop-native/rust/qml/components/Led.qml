import QtQuick
import com.yap.native

// A status LED (ui/led.tsx): 6 px, `line-strong` when off. A lit LED gets
// the halo (2 px ring at 22 %, soft 8 px disc at 45 %); only opacity moves.
Item {
    id: root

    property color color: Theme.talk
    property bool lit: true
    property int size: Theme.led

    implicitWidth: size
    implicitHeight: size

    Rectangle {
        anchors.centerIn: parent
        width: root.size + 8
        height: width
        radius: width / 2
        color: Qt.rgba(root.color.r, root.color.g, root.color.b, 0.45 * 0.35)
        opacity: root.lit ? 1 : 0
        Behavior on opacity { OpacityAnimator { duration: root.lit ? Theme.speakOn : Theme.speakOff } }
    }
    Rectangle {
        anchors.centerIn: parent
        width: root.size + 4
        height: width
        radius: width / 2
        color: Qt.rgba(root.color.r, root.color.g, root.color.b, 0.22)
        opacity: root.lit ? 1 : 0
        Behavior on opacity { OpacityAnimator { duration: root.lit ? Theme.speakOn : Theme.speakOff } }
    }
    Rectangle {
        anchors.fill: parent
        radius: width / 2
        color: Theme.lineStrong
    }
    Rectangle {
        anchors.fill: parent
        radius: width / 2
        color: root.color
        opacity: root.lit ? 1 : 0
        Behavior on opacity { OpacityAnimator { duration: root.lit ? Theme.speakOn : Theme.speakOff } }
    }
}
