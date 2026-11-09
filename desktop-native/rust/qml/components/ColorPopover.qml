pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import Yap.Chrome
import com.yap.native

// The colour picker (what Chromium's popup gives the web's
// `<input type="color">`): a saturation/brightness square, a hue slider,
// the hex field, the old and new colour, an eyedropper for anything on
// screen, and the last 8 colours picked (Colors.recents,
// `color-recents.json`). Declare it inside the key that opens it; it opens
// under the key (above it when there is no room) with `request(value)`.
//
// The colour applies as it changes (`picked` on every new colour, as the
// web input's `input` events do). Esc puts back the colour it opened with
// and closes; a click outside keeps the new one, which goes to the
// recents. With YAP_PICK_COLOR set (tests) `request` applies that colour
// at once ("-": nothing) and never opens.
T.Popup {
    id: root

    // Lowercase noun for the title and names ("banner color").
    property string name
    // Where the picker starts when the value is empty or not a colour.
    property string start: "#000000"

    // Every new colour, `#RRGGBB`; on Esc, the value it opened with (which
    // may be empty).
    signal picked(string value)

    // The value it opened with, as the caller had it.
    property string initial: ""
    readonly property string initialHex: Colors.normalize(initial)
    property real hue: 0
    property real sat: 0
    property real val: 0
    readonly property string current: Colors.hsvToHex(hue, sat, val)
    // The colour last sent with `picked`.
    property string sent: ""
    property bool reverting: false
    property bool sampling: false
    readonly property var recents: JSON.parse(Colors.recents || "[]")

    function request(value) {
        const forced = Files.testPickColor()
        if (forced.length > 0) {
            const hex = Colors.normalize(forced)
            if (hex.length > 0) {
                root.picked(hex)
                Colors.remember(hex)
            }
            return
        }
        initial = value
        sent = initialHex
        const from = initialHex.length > 0 ? initialHex : Colors.normalize(start)
        hue = 0
        sat = 0
        val = 0
        setHex(from.length > 0 ? from : "#000000", false, true)
        place()
        open()
    }

    // Takes `hex` as the colour: a grey keeps the hue it had, black the
    // saturation too, as Chromium's picker does.
    function setHex(hex, apply, syncField) {
        const hsv = Colors.hexToHsv(hex)
        if (hsv.length === 0)
            return
        const [h, s, v] = JSON.parse(hsv)
        if (s > 0 && v > 0)
            hue = h
        if (v > 0)
            sat = s
        val = v
        if (syncField)
            hexField.text = current.slice(1)
        if (apply)
            send()
    }

    // The square, the slider or a key moved the colour.
    function moved() {
        hexField.text = current.slice(1)
        send()
    }

    function send() {
        if (current === sent)
            return
        sent = current
        root.picked(current)
    }

    function revert() {
        reverting = true
        if (sent !== initialHex)
            root.picked(initial)
        close()
    }

    // Under the key, or over it when the window has no room below.
    function place() {
        const key = root.parent
        const win = key ? key.Window.window : null
        if (!key || !win)
            return
        // The column has no height until it shows: its rows' sum.
        const tall = topPadding + bottomPadding + 17 + 144 + 16 + 32 + 3 * 12
            + (recents.length > 0 ? 12 + 16 + 6 + 20 : 0)
        const at = key.mapToItem(null, 0, 0)
        const below = at.y + key.height + 8 + tall <= win.height - 8
        y = below ? key.height + 8 : -tall - 8
        x = Math.min(0, win.width - 8 - (at.x + width))
        transformOrigin = below ? T.Popup.TopLeft : T.Popup.BottomLeft
    }

    width: 248
    // A template popup sizes to nothing on its own.
    implicitHeight: contentItem.implicitHeight + topPadding + bottomPadding
    padding: 12
    topPadding: 14
    focus: true
    // Esc is ours (revert), not the popup's.
    closePolicy: T.Popup.CloseOnPressOutside
    transformOrigin: T.Popup.TopLeft
    enter: PresenceIn { duration: Theme.durPopover; fromScale: Theme.scalePopover }
    exit: PresenceOut { duration: Theme.durPopoverExit }

    onOpened: {
        hexField.forceActiveFocus()
        hexField.selectAll()
    }
    onClosed: {
        if (sampling)
            Eyedropper.cancel()
        sampling = false
        if (!reverting && sent.length > 0 && sent !== initialHex)
            Colors.remember(sent)
        reverting = false
    }

    Connections {
        target: Eyedropper
        enabled: root.sampling
        function onPicked(hex) {
            root.sampling = false
            root.setHex(hex, true, true)
        }
        function onCancelled() {
            root.sampling = false
        }
    }

    background: Rectangle {
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        FloatShadow {}
        Rectangle { width: parent.width; height: 2; color: Theme.ink1 }
    }

    contentItem: FocusScope {
        implicitHeight: column.implicitHeight
        Accessible.role: Accessible.Dialog
        Accessible.name: title.text

        Keys.onEscapePressed: event => {
            event.accepted = true
            root.revert()
        }

        Column {
            id: column
            width: parent.width
            spacing: 12

            Text {
                id: title
                width: parent.width
                height: 17
                verticalAlignment: Text.AlignVCenter
                textFormat: Text.PlainText
                text: qsTr("Custom %1").arg(root.name)
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.occupant
            }

            // Saturation left to right, brightness bottom to top, over the hue.
            Item {
                id: square
                width: parent.width
                height: 144
                activeFocusOnTab: true
                // The focus ring is for the keyboard: a press focuses it
                // without one.
                property bool keyed: false
                onActiveFocusChanged: keyed = activeFocus && !area.pressed
                Accessible.role: Accessible.Slider
                Accessible.name: qsTr("Saturation and brightness")
                Accessible.description: qsTr("Saturation %1%, brightness %2%")
                    .arg(Math.round(root.sat * 100)).arg(Math.round(root.val * 100))

                function pointAt(px, py) {
                    root.sat = Math.max(0, Math.min(1, px / width))
                    root.val = 1 - Math.max(0, Math.min(1, py / height))
                    root.moved()
                }

                Keys.onPressed: event => {
                    const step = (event.modifiers & Qt.ShiftModifier) ? 0.1 : 0.01
                    let s = root.sat
                    let v = root.val
                    switch (event.key) {
                    case Qt.Key_Left: s -= step; break
                    case Qt.Key_Right: s += step; break
                    case Qt.Key_Up: v += step; break
                    case Qt.Key_Down: v -= step; break
                    case Qt.Key_PageUp: v += 0.1; break
                    case Qt.Key_PageDown: v -= 0.1; break
                    case Qt.Key_Home: s = 0; break
                    case Qt.Key_End: s = 1; break
                    default: return
                    }
                    event.accepted = true
                    square.keyed = true
                    // Whole steps, so keys land on round values.
                    root.sat = Math.max(0, Math.min(1, Math.round(s * 100) / 100))
                    root.val = Math.max(0, Math.min(1, Math.round(v * 100) / 100))
                    root.moved()
                }

                Rectangle {
                    anchors.fill: parent
                    radius: Theme.radius
                    gradient: Gradient {
                        orientation: Gradient.Horizontal
                        GradientStop { position: 0; color: "#ffffff" }
                        GradientStop { position: 1; color: Colors.hsvToHex(root.hue, 1, 1) }
                    }
                }
                Rectangle {
                    anchors.fill: parent
                    radius: Theme.radius
                    gradient: Gradient {
                        GradientStop { position: 0; color: "#00000000" }
                        GradientStop { position: 1; color: "#ff000000" }
                    }
                }
                Rectangle {
                    anchors.fill: parent
                    radius: Theme.radius
                    color: "transparent"
                    border.width: 1
                    border.color: Theme.lineStrong
                }
                Rectangle {
                    anchors.fill: parent
                    anchors.margins: -3
                    visible: square.activeFocus && square.keyed
                    radius: Theme.radius + 3
                    color: "transparent"
                    border.width: 2
                    border.color: Theme.focus
                }

                Ring {
                    x: Math.round(root.sat * square.width - width / 2)
                    y: Math.round((1 - root.val) * square.height - height / 2)
                    fill: root.current
                    big: area.pressed
                }

                MouseArea {
                    id: area
                    anchors.fill: parent
                    // The handle reaches the edges; so does the pointer.
                    anchors.margins: -6
                    preventStealing: true
                    cursorShape: Qt.CrossCursor
                    onPressed: mouse => {
                        square.forceActiveFocus()
                        square.pointAt(mouse.x - 6, mouse.y - 6)
                    }
                    onPositionChanged: mouse => {
                        if (pressed)
                            square.pointAt(mouse.x - 6, mouse.y - 6)
                    }
                }
            }

            // The hue, red to red.
            T.Slider {
                id: hueSlider
                width: parent.width
                height: 16
                from: 0
                to: 360
                stepSize: 1
                live: true
                value: root.hue
                focusPolicy: Qt.StrongFocus
                Accessible.role: Accessible.Slider
                Accessible.name: qsTr("Hue")
                Accessible.description: qsTr("%1 degrees").arg(Math.round(root.hue))
                onMoved: {
                    root.hue = value
                    root.moved()
                }
                Keys.onPressed: event => {
                    const step = (event.modifiers & Qt.ShiftModifier) ? 10 : 1
                    let h = root.hue
                    switch (event.key) {
                    case Qt.Key_Left:
                    case Qt.Key_Down: h -= step; break
                    case Qt.Key_Right:
                    case Qt.Key_Up: h += step; break
                    case Qt.Key_PageDown: h -= 30; break
                    case Qt.Key_PageUp: h += 30; break
                    case Qt.Key_Home: h = 0; break
                    case Qt.Key_End: h = 360; break
                    default: return
                    }
                    event.accepted = true
                    root.hue = Math.max(0, Math.min(360, Math.round(h)))
                    root.moved()
                }

                background: Rectangle {
                    x: 0
                    y: (hueSlider.height - height) / 2
                    width: hueSlider.width
                    height: 12
                    radius: 6
                    border.width: 1
                    border.color: Theme.lineStrong
                    gradient: Gradient {
                        orientation: Gradient.Horizontal
                        GradientStop { position: 0 / 6; color: "#ff0000" }
                        GradientStop { position: 1 / 6; color: "#ffff00" }
                        GradientStop { position: 2 / 6; color: "#00ff00" }
                        GradientStop { position: 3 / 6; color: "#00ffff" }
                        GradientStop { position: 4 / 6; color: "#0000ff" }
                        GradientStop { position: 5 / 6; color: "#ff00ff" }
                        GradientStop { position: 6 / 6; color: "#ff0000" }
                    }
                }
                handle: Ring {
                    x: Math.round(hueSlider.visualPosition * (hueSlider.width - 12) + 6 - width / 2)
                    y: Math.round((hueSlider.height - height) / 2)
                    fill: Colors.hsvToHex(root.hue, 1, 1)
                    big: hueSlider.pressed
                    focused: hueSlider.visualFocus
                }
                HoverHandler { cursorShape: Qt.PointingHandCursor }
            }

            // The eyedropper, the hex, and the old colour beside the new.
            Row {
                width: parent.width
                spacing: 8

                IconKey {
                    id: eyedropperKey
                    iconName: "eyedropper"
                    size: 32
                    tone: root.sampling ? "toggled" : "rest"
                    tip: qsTr("Pick a colour from the screen")
                    onClicked: {
                        if (root.sampling)
                            return
                        root.sampling = true
                        Eyedropper.start()
                    }
                }

                KitField {
                    id: hexField
                    readonly property bool complete: text.length === 6 || text.length === 3
                    width: parent.width - eyedropperKey.width - preview.width - 16
                    small: true
                    leadingInset: 10
                    // Room for a pasted "#33ccff" before it is cleaned.
                    maximumLength: 16
                    font: Theme.type.code
                    inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhPreferUppercase
                    invalid: text.length > 0 && !complete && !activeFocus
                    Accessible.name: qsTr("Hex %1").arg(root.name)
                    // Hex digits only, 6 at most: a paste with "#" or spaces
                    // is cleaned, then taken when it is whole.
                    onTextEdited: {
                        const clean = text.replace(/[^0-9a-fA-F]/g, "").slice(0, 6).toUpperCase()
                        if (clean !== text) {
                            const at = cursorPosition
                            text = clean
                            cursorPosition = Math.min(at, clean.length)
                        }
                        if (clean.length === 6)
                            root.setHex("#" + clean, true, false)
                    }
                    // Focus left (not a close or Esc): a short hex counts.
                    onEditingFinished: {
                        if (!root.opened)
                            return
                        if (complete)
                            root.setHex("#" + text, true, false)
                        text = root.current.slice(1)
                    }
                    onAccepted: {
                        if (complete)
                            root.setHex("#" + text, true, false)
                        root.close()
                    }
                    Text {
                        x: 10
                        anchors.verticalCenter: parent.verticalCenter
                        text: "#"
                        color: Theme.ink3
                        font: Theme.type.code
                    }
                }

                // Old | new. The old half puts the opening colour back.
                Item {
                    id: preview
                    width: 48
                    height: 32
                    Rectangle {
                        id: chip
                        anchors.fill: parent
                        radius: 4
                        color: Theme.tint1
                        border.width: 1
                        border.color: Theme.lineStrong
                    }
                    T.AbstractButton {
                        id: oldHalf
                        x: 1
                        y: 1
                        width: parent.width / 2 - 1
                        height: parent.height - 2
                        focusPolicy: Qt.StrongFocus
                        enabled: root.sent !== root.initialHex
                        Accessible.role: Accessible.Button
                        Accessible.name: root.initialHex.length > 0
                            ? qsTr("Original colour %1, put it back").arg(root.initialHex)
                            : qsTr("Put back the original %1").arg(root.name)
                        background: Item {
                            clip: true
                            Rectangle {
                                anchors.fill: parent
                                anchors.rightMargin: -4
                                radius: 3
                                color: root.initialHex.length > 0 ? root.initialHex : Theme.tint1
                            }
                            // None (Auto): a slash, as an empty swatch.
                            Rectangle {
                                visible: root.initialHex.length === 0
                                anchors.centerIn: parent
                                width: 1.5
                                height: Math.hypot(parent.width, parent.height)
                                rotation: 36
                                color: Theme.ink3
                                antialiasing: true
                            }
                        }
                        HoverHandler { cursorShape: oldHalf.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor }
                        onClicked: {
                            if (root.initialHex.length > 0)
                                root.setHex(root.initialHex, false, true)
                            root.sent = root.initialHex
                            root.picked(root.initial)
                        }
                    }
                    Item {
                        x: parent.width / 2
                        y: 1
                        width: parent.width / 2 - 1
                        height: parent.height - 2
                        clip: true
                        Accessible.role: Accessible.StaticText
                        Accessible.name: qsTr("New colour %1").arg(root.current)
                        Rectangle {
                            anchors.fill: parent
                            anchors.leftMargin: -4
                            radius: 3
                            color: root.current
                        }
                    }
                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: -3
                        visible: oldHalf.visualFocus
                        radius: 6
                        color: "transparent"
                        border.width: 2
                        border.color: Theme.focus
                    }
                }
            }

            // The last colours picked.
            Column {
                visible: root.recents.length > 0
                width: parent.width
                spacing: 6
                Text {
                    height: 16
                    verticalAlignment: Text.AlignVCenter
                    textFormat: Text.PlainText
                    text: qsTr("Recent")
                    color: Theme.ink3
                    font: Theme.type.captionStrong
                }
                Row {
                    spacing: 8
                    Accessible.role: Accessible.Grouping
                    Accessible.name: qsTr("Recent colours")
                    Repeater {
                        model: root.recents
                        delegate: T.AbstractButton {
                            id: recent
                            required property string modelData
                            width: 20
                            height: 20
                            focusPolicy: Qt.StrongFocus
                            scale: down ? Theme.scalePress : 1
                            Behavior on scale { NumberAnimation { duration: Theme.dur1; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
                            Accessible.role: Accessible.Button
                            Accessible.name: modelData
                            background: Item {
                                Rectangle {
                                    anchors.fill: parent
                                    radius: 4
                                    color: recent.modelData
                                    border.width: 1
                                    border.color: Theme.lineStrong
                                }
                                Rectangle {
                                    anchors.fill: parent
                                    anchors.margins: -3
                                    visible: recent.visualFocus || recent.modelData === root.current
                                    radius: 6
                                    color: "transparent"
                                    border.width: 2
                                    border.color: Theme.accentInk
                                }
                            }
                            HoverHandler { cursorShape: Qt.PointingHandCursor }
                            onClicked: root.setHex(modelData, true, true)
                        }
                    }
                }
            }
        }
    }

    // A handle: the colour in a white ring with a dark hairline, a size up
    // while dragged.
    component Ring: Rectangle {
        id: ring
        property color fill
        property bool big: false
        property bool focused: false
        width: big ? 18 : 16
        height: width
        radius: width / 2
        color: Qt.rgba(0, 0, 0, 0.35)
        Behavior on width { NumberAnimation { duration: Theme.dur1; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
        Rectangle {
            anchors.fill: parent
            anchors.margins: 1
            radius: width / 2
            color: ring.fill
            border.width: 2
            border.color: "#ffffff"
        }
        Rectangle {
            anchors.fill: parent
            anchors.margins: -3
            visible: ring.focused
            radius: width / 2
            color: "transparent"
            border.width: 2
            border.color: Theme.focus
        }
    }
}
