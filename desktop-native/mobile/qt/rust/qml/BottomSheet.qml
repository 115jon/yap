import QtQuick
import Yap.Mobile
import Yap.Ui

// A bottom sheet (DESIGN.md "Dialogs, Sheets, Toasts": on a phone dialogs
// and menus are sheets): a float ply with a 4 px top corner over a scrim.
// Any phone surface can use it; put it in a full-window item and give it
// content:
//
//   BottomSheet {
//       id: sheet
//       anchors.fill: parent
//       title: qsTr("Delete message?")       // optional
//       contentHeight: column.implicitHeight
//       Column { id: column; width: parent.width; ... }
//   }
//   sheet.open()
//
// - It is as tall as its content when that fits in 80 % of the window.
//   Taller content rests at 60 % and a pull up takes it to full height
//   (under the status bar); `startFull` opens it there.
// - A drag down closes it (a fling does from anywhere), a tap on the scrim
//   closes it, and so does Back. The predictive back gesture moves it
//   before it commits (Native's back layers).
// - `flickable`: a list or Flickable inside that scrolls the content. It
//   scrolls only at the sheet's top detent, and a pull down from its top
//   edge moves the sheet instead (its top edge is the sheet's).
// - The keyboard pushes it up (Native.imeHeight); opening it hides a
//   keyboard that was up for something else.
FocusScope {
    id: sheet

    default property alias content: body.data
    property string title: ""
    // The content's height. The body is this tall, or what fits.
    property real contentHeight: 0
    property Flickable flickable: null
    property bool startFull: false
    // Light system bar icons while it is up (for a ground darker than the
    // themes' scrim leaves it).
    property bool darkBars: false

    // Up, or on its way in or out. Its own word: `visible` also goes false
    // when something above the sheet hides.
    property bool up: false
    readonly property bool opened: up
    signal closed()

    function open() {
        if (up && !closing)
            return
        closing = false
        back = 0
        pull = 0
        up = true
        Qt.inputMethod.hide()
        forceActiveFocus()
        Native.pushBackLayer(sheet)
        if (darkBars)
            Native.holdDarkBars(sheet)
        detent = startFull ? 2 : 1
        settle.duration = Theme.durSheet
        settle.to = detentExtent(detent)
        settle.restart()
    }

    function close() {
        if (!up || closing)
            return
        closing = true
        detent = 0
        Native.popBackLayer(sheet)
        Native.releaseDarkBars(sheet)
        Qt.inputMethod.hide()
        settle.duration = Theme.dur4Exit
        settle.to = 0
        settle.restart()
    }

    // Native's back layer: the gesture's progress, its cancel, its commit.
    function backProgressed(progress) {
        backEase.stop()
        back = progress
    }
    function backCancelled() {
        backEase.restart()
    }
    function backInvoked() {
        close()
    }

    visible: up
    z: 100
    // Hidden from above while open (the page under it went away): gone at
    // once, and Back is given back.
    onVisibleChanged: if (!visible && up) {
        settle.stop()
        Native.popBackLayer(sheet)
        Native.releaseDarkBars(sheet)
        up = false
        closing = false
        detent = 0
        extent = 0
        pull = 0
        closed()
    }
    Component.onDestruction: {
        Native.popBackLayer(sheet)
        Native.releaseDarkBars(sheet)
    }

    // ---------- Geometry ----------
    readonly property real topGap: SafeArea.margins.top + 8
    readonly property real bottomPad: Math.max(SafeArea.margins.bottom, Native.gestureInset, Native.imeHeight)
    readonly property real chrome: 20 + (title.length > 0 ? titleText.height + 12 : 0)
    readonly property real natural: chrome + contentHeight + bottomPad
    // The top detent: everything, or the window under the status bar.
    readonly property real full: Math.min(natural, height - topGap)
    // The resting detent: everything when it fits in 80 % of the window (a
    // menu's last row, often the destructive one, is not left under the
    // fold), else 60 % of the window; with the keyboard up, the top. So a
    // sheet whose content grows past 80 % while it is open drops to 60 %.
    readonly property real rest: Native.imeHeight > 0 || full <= Math.round(height * 0.8)
        ? full : Math.min(full, Math.round(height * 0.6))
    readonly property bool expandable: full > rest + 1
    readonly property bool atTop: detent === 2 || (detent === 1 && !expandable)

    // 0 closed, 1 resting, 2 full.
    property int detent: 0
    property bool closing: false
    // How much of the sheet shows, in px; `pull` is the inner list's drag
    // past its top edge, `back` the back gesture's progress.
    property real extent: 0
    property real pull: 0
    property real back: 0
    readonly property real shown: Math.max(0, extent - pull)

    readonly property bool dragging: drag.active || gripDrag.active
    // A drag's speed, from its last steps: `at` grows as the sheet goes
    // down. A finger that stopped before lifting did not fling.
    property real trackAt: 0
    property double trackTime: 0
    property real trackSpeed: 0
    function track(at) {
        const now = Date.now()
        if (now > trackTime)
            trackSpeed = (at - trackAt) / (now - trackTime) * 1000
        trackAt = at
        trackTime = now
    }
    function flung() {
        return Date.now() - trackTime > 80 ? 0 : trackSpeed
    }

    property real dragFrom: 0
    function dragBegan() {
        settle.stop()
        dragFrom = extent
        trackAt = 0
        trackTime = Date.now()
        trackSpeed = 0
    }
    function dragMoved(dy) {
        track(dy)
        extent = Math.max(0, Math.min(full, dragFrom - dy))
    }
    function dragEnded() {
        release(extent, flung())
    }

    function detentExtent(d) {
        return d === 2 ? full : d === 1 ? rest : 0
    }
    // Where a drag let go at `at` px with `velocity` px/s (down positive)
    // ends: a fling picks the next detent that way, else the nearest.
    function release(at, velocity) {
        let to
        if (velocity > 700)
            to = expandable && at > rest ? 1 : 0
        else if (velocity < -700)
            to = expandable ? 2 : 1
        else if (at < rest * 0.6)
            to = 0
        else
            to = expandable && at > (rest + full) / 2 ? 2 : 1
        if (to === 0) {
            close()
            return
        }
        detent = to
        settle.duration = Theme.dur3
        settle.to = detentExtent(to)
        settle.restart()
    }
    // The content, the window or the keyboard changed under an open sheet.
    function refit() {
        if (!up || closing || dragging || pull > 0)
            return
        if (detent === 2 && !expandable)
            detent = 1
        settle.duration = Theme.dur2
        settle.to = detentExtent(detent)
        settle.restart()
    }
    onRestChanged: refit()
    onFullChanged: refit()

    NumberAnimation {
        id: settle
        target: sheet
        property: "extent"
        easing.type: Easing.BezierSpline
        easing.bezierCurve: Theme.easeDrawer
        onFinished: if (sheet.closing) {
            sheet.up = false
            sheet.closing = false
            sheet.closed()
        }
    }
    NumberAnimation {
        id: backEase
        target: sheet
        property: "back"
        to: 0
        duration: Theme.dur2
        easing.type: Easing.BezierSpline
        easing.bezierCurve: Theme.easeOut
    }

    // ---------- Scrim ----------
    Rectangle {
        anchors.fill: parent
        color: Theme.scrim
        opacity: Math.min(1, sheet.shown / Math.max(1, sheet.rest)) * (1 - sheet.back * 0.4)
    }
    // Nothing under the sheet takes a touch; a tap off it closes it.
    MouseArea {
        anchors.fill: parent
        preventStealing: true
        onClicked: sheet.close()
    }

    // ---------- The ply ----------
    Rectangle {
        id: panel
        // One px wider each side: only the top edge of the seam shows.
        x: -1
        width: parent.width + 2
        y: parent.height - sheet.shown + sheet.back * 48
        // Past the window's foot, so a pull up never shows under it.
        height: sheet.full + 64
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        topLeftRadius: Theme.radiusSheet
        topRightRadius: Theme.radiusSheet
        transformOrigin: Item.Bottom
        scale: 1 - sheet.back * 0.04

        // Touches on the ply stay on it.
        MouseArea { anchors.fill: parent }

        // A drag on the ply moves the sheet, unless an inner list has the
        // body (at the top detent); the handle and the title always do.
        DragHandler {
            id: drag
            target: null
            xAxis.enabled: false
            enabled: !sheet.closing && (sheet.flickable === null || !sheet.atTop)
            onActiveChanged: active ? sheet.dragBegan() : sheet.dragEnded()
            onActiveTranslationChanged: if (active) sheet.dragMoved(activeTranslation.y)
        }
        Item {
            width: parent.width
            height: sheet.chrome
            DragHandler {
                id: gripDrag
                target: null
                enabled: !sheet.closing
                xAxis.enabled: false
                onActiveChanged: active ? sheet.dragBegan() : sheet.dragEnded()
                onActiveTranslationChanged: if (active) sheet.dragMoved(activeTranslation.y)
            }
        }

        // The handle (a squared grip).
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            y: 8
            width: 32
            height: 4
            radius: Theme.radius
            color: Theme.inkOff
        }

        Text {
            id: titleText
            visible: sheet.title.length > 0
            x: 1 + 16 + sheet.SafeArea.margins.left
            y: 24
            width: sheet.width - 32 - sheet.SafeArea.margins.left - sheet.SafeArea.margins.right
            text: sheet.title
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: Theme.ink1
            // DESIGN.md: a sheet's title is 22 px Bricolage.
            font.family: Theme.type.sectionTitle.family
            font.weight: Theme.type.sectionTitle.weight
            font.pixelSize: Math.round(22 * Theme.textScale)
            font.letterSpacing: -0.22 * Theme.textScale
            Accessible.role: Accessible.Heading
        }

        Item {
            id: body
            x: 1 + sheet.SafeArea.margins.left
            y: sheet.chrome
            width: sheet.width - sheet.SafeArea.margins.left - sheet.SafeArea.margins.right
            height: Math.max(0, sheet.full - sheet.chrome - sheet.bottomPad)
            clip: true
        }
    }

    // ---------- The inner list ----------
    // It scrolls at the top detent only. A drag past its top edge there is
    // reported as overshoot with the content held in place (StopAtBounds),
    // and moves the sheet by that much; letting go settles the sheet.
    Binding {
        target: sheet.flickable
        when: sheet.flickable !== null
        property: "interactive"
        value: sheet.atTop && !sheet.dragging && !sheet.closing
    }
    Binding {
        target: sheet.flickable
        when: sheet.flickable !== null
        property: "boundsBehavior"
        value: Flickable.DragOverBounds
    }
    Binding {
        target: sheet.flickable
        when: sheet.flickable !== null
        property: "boundsMovement"
        value: Flickable.StopAtBounds
    }
    // Content that fits still takes the drag, so its overshoot moves the
    // sheet (a Flickable's default only flicks what overflows).
    Binding {
        target: sheet.flickable
        when: sheet.flickable !== null
        property: "flickableDirection"
        value: Flickable.VerticalFlick
    }
    Connections {
        target: sheet.flickable
        enabled: sheet.up && !sheet.closing
        function onVerticalOvershootChanged() {
            const list = sheet.flickable
            if (!list.dragging)
                return
            const at = Math.max(0, -list.verticalOvershoot)
            sheet.track(at)
            sheet.pull = at
        }
        function onDraggingChanged() {
            if (sheet.flickable.dragging || sheet.pull <= 0)
                return
            // The pull becomes the sheet's own position, in one step.
            sheet.extent = sheet.shown
            sheet.pull = 0
            sheet.release(sheet.extent, sheet.flung())
        }
    }
}
