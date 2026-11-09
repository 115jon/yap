import QtQuick
import QtQuick.Templates as T
import com.yap.native

// The volume slider (AudioDeviceMenu.tsx VolumeSlider, ContextMenuItems
// Slider): the label and the Martian percentage over a 4 px tint-3 track
// with the primary-fill range and a 14 px thumb ringed in the panel ply.
// `moved` carries whole percents.
//
// `boost: true` is the per-user volume (0-400 %): a tick marks 100 %, the
// range past 200 % lights idle amber (DESIGN.md: amber is the warning
// light), and so does the percentage. A drag snaps to 100 % within a few
// pixels of it; the keys step 5 % (PageUp/PageDown 25 %, Home/End the
// ends). While it moves it emits `percentMoved` (apply it live, cheaply)
// and, on release or 400 ms after the last key, `committed` (save it); the
// handle follows `pending` meanwhile, so `value` stays the caller's binding.
T.Slider {
    id: control

    property string label
    property bool boost: false

    // The percent a boost slider shows before the caller's `value` catches
    // up (NaN: none).
    property real pending: NaN
    readonly property real shown: isNaN(pending) ? value : pending
    readonly property bool hot: boost && shown > 200

    // Within this many pixels of 100 % a drag lands on it.
    readonly property real snapPixels: 6

    signal percentMoved(int percent)
    signal committed(int percent)

    function commit() {
        commitTimer.stop();
        if (isNaN(pending))
            return;
        const percent = Math.round(pending);
        // The caller saves it and its `value` binding follows.
        committed(percent);
        pending = NaN;
    }

    function preview(v) {
        pending = Math.max(from, Math.min(to, v));
        percentMoved(Math.round(pending));
    }

    function positionOf(v) {
        return to > from ? Math.max(0, Math.min(1, (v - from) / (to - from))) : 0;
    }

    from: 0
    to: boost ? 400 : 200
    stepSize: 1
    snapMode: T.Slider.SnapAlways
    implicitWidth: 232
    implicitHeight: 56
    leftPadding: 8
    rightPadding: 8
    topPadding: 30
    bottomPadding: 12
    focusPolicy: Qt.StrongFocus

    Accessible.role: Accessible.Slider
    Accessible.name: label

    Connections {
        target: control
        enabled: control.boost
        function onMoved() {
            let v = control.value;
            const track = control.availableWidth - control.handle.width;
            const perPixel = track > 0 ? (control.to - control.from) / track : 0;
            if (control.pressed && Math.abs(v - 100) <= control.snapPixels * perPixel)
                v = 100;
            control.preview(v);
            if (!control.pressed)
                commitTimer.restart();
        }
        function onPressedChanged() {
            if (!control.pressed)
                control.commit();
        }
    }

    Keys.onPressed: (event) => {
        if (!control.boost) {
            event.accepted = false;
            return;
        }
        const now = control.shown;
        // To the next whole step in that direction, so 103 goes to 105.
        const up = (s) => (Math.floor(now / s + 1e-6) + 1) * s;
        const down = (s) => (Math.ceil(now / s - 1e-6) - 1) * s;
        let next;
        switch (event.key) {
        case Qt.Key_Right:
        case Qt.Key_Up: next = up(5); break;
        case Qt.Key_Left:
        case Qt.Key_Down: next = down(5); break;
        case Qt.Key_PageUp: next = up(25); break;
        case Qt.Key_PageDown: next = down(25); break;
        case Qt.Key_Home: next = control.from; break;
        case Qt.Key_End: next = control.to; break;
        default:
            event.accepted = false;
            return;
        }
        event.accepted = true;
        control.preview(next);
        commitTimer.restart();
    }

    Timer {
        id: commitTimer
        interval: 400
        onTriggered: control.commit()
    }

    Component.onDestruction: commit()

    Text {
        x: control.leftPadding
        y: 6
        textFormat: Text.PlainText
        text: control.label
        color: Theme.ink3
        font: Theme.type.captionStrong
    }
    Text {
        anchors.right: parent.right
        anchors.rightMargin: control.rightPadding
        y: 6
        textFormat: Text.PlainText
        text: qsTr("%1%").arg(Math.round(control.shown))
        color: control.hot ? Theme.warningInk : Theme.ink1
        font: Theme.type.numSm
    }

    background: Item {
        id: track
        x: control.leftPadding
        y: control.topPadding + (control.availableHeight - height) / 2
        width: control.availableWidth
        height: 4
        // The handle's centre for a percent, in the track's coordinates.
        function centreOf(v) {
            return control.handle.width / 2 + control.positionOf(v) * (width - control.handle.width);
        }
        Rectangle {
            anchors.fill: parent
            radius: 2
            color: Theme.tint3
        }
        Rectangle {
            width: control.handle.x + control.handle.width / 2 - control.leftPadding
            height: parent.height
            radius: 2
            color: Theme.primaryFill
        }
        // Past 200 %: the fill lights amber.
        Rectangle {
            visible: control.hot
            x: track.centreOf(200)
            width: Math.max(0, control.handle.x + control.handle.width / 2 - control.leftPadding - x)
            height: parent.height
            radius: 2
            color: Theme.warningInk
        }
        // 100 %, normal: a tick across the track.
        Rectangle {
            visible: control.boost
            x: Math.round(track.centreOf(100) - width / 2)
            y: -3
            width: 2
            height: parent.height + 6
            radius: 1
            color: Theme.ink3
        }
    }

    handle: Rectangle {
        x: control.leftPadding + control.positionOf(control.shown) * (control.availableWidth - width)
        y: control.topPadding + (control.availableHeight - height) / 2
        width: 14
        height: 14
        radius: 7
        color: control.hot ? Theme.warningInk : Theme.primaryFill
        border.width: 2
        border.color: Theme.ply3
        Rectangle {
            anchors.fill: parent
            anchors.margins: -4
            visible: control.visualFocus
            color: "transparent"
            radius: width / 2
            border.width: 2
            border.color: Theme.focus
        }
    }

    HoverHandler { cursorShape: Qt.PointingHandCursor }
}
