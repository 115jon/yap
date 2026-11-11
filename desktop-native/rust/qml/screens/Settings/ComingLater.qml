import QtQuick
import com.yap.native
import Yap.Ui

// A status word for rows whose control hasn't shipped yet
// (primitives.tsx ComingLater): an unlit LED and "Coming later".
LedLabel {
    text: qsTr("Coming later")
    ledState: "off"
    plain: true
}
