pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Edit profile, a page over Settings (the desktop's ProfileEditor, phone
// layout, on the same Yap.Ui ProfileDraft: its checks, limits and saves).
// On top, the draft as others will see it: the banner, the avatar over it
// and the name; a tap on either picks a new one. Then numbered sections:
// Avatar (choose from the gallery, frame it, remove it), Banner (choose,
// remove), About you (display name, pronouns, custom status, about me,
// each with its count). Pictures come from Android's photo picker (no
// storage permission); a new avatar is framed first (AvatarFramePage). Once
// anything changed, Cancel and Save ride above the keyboard; Back while
// edits are unsaved asks before it drops them. The profile colours, the
// name style and the collectibles stay on the desktop.
Rectangle {
    id: root

    readonly property bool sheet: false
    readonly property bool isProfileEditor: true
    property real backProgress: 0

    signal closeRequested()

    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: Math.max(SafeArea.margins.bottom, Native.gestureInset)
    // The keyboard, or the system's bottom edge.
    readonly property real keyboardPad: Math.max(bottomInset, Native.imeHeight)
    readonly property bool barShown: draft.dirty || draft.saving || draft.error.length > 0
    readonly property real bottomSpace: keyboardPad + (barShown ? bar.height : 0)

    // Which picture the photo picker is choosing (only this page takes the
    // result), and an avatar over its limit (the too-big card's JSON).
    property string picking: ""
    property string avatarTooBig: ""
    // The field being typed in, kept in view above the keyboard.
    property Item typing: null

    color: Theme.ply0
    transform: [
        Scale {
            origin.x: root.width / 2
            origin.y: root.height / 2
            xScale: 1 - root.backProgress * 0.08
            yScale: 1 - root.backProgress * 0.08
        },
        Translate { x: root.backProgress * root.width * 0.1 }
    ]

    ProfileDraft {
        id: draft
        active: true
        editsName: true
    }
    Component.onCompleted: draft.reset()

    // Unsaved edits hold Back: it asks first. A save on its way does not
    // (it lands without the page).
    readonly property bool guarding: draft.dirty && !draft.saving
    onGuardingChanged: guarding ? Native.pushBackLayer(root) : Native.popBackLayer(root)
    Component.onDestruction: Native.popBackLayer(root)
    function backProgressed(progress) {}
    function backCancelled() {}
    function backInvoked() {
        requestBack()
    }
    function requestBack() {
        if (!guarding) {
            root.closeRequested()
            return
        }
        Qt.inputMethod.hide()
        discardSheet.open()
    }

    function choose(kind) {
        if (draft.saving)
            return
        Qt.inputMethod.hide()
        root.picking = kind
        Native.pickImage(kind === "avatar" ? Settings.avatarLimit : Settings.bannerLimit)
    }
    Connections {
        target: Native
        function onMediaPicked(paths, failed) {
            const kind = root.picking
            if (kind.length === 0)
                return
            root.picking = ""
            if (paths.length === 0) {
                if (failed > 0)
                    draft.localError = qsTr("The picked image could not be read.")
                return
            }
            const path = paths[0]
            const tooBig = draft.check(kind, path)
            if (kind === "avatar")
                root.avatarTooBig = tooBig ?? ""
            if (tooBig === null || tooBig.length > 0)
                return
            if (kind === "avatar") {
                draft.localError = ""
                framer.open(path, draft.fileUrl(path), null)
            } else {
                draft.takeBanner(path)
            }
        }
        function onImeHeightChanged() {
            if (root.typing)
                Qt.callLater(root.reveal, root.typing)
        }
    }
    function save() {
        // A word still being composed joins the text first.
        Qt.inputMethod.commit()
        root.avatarTooBig = ""
        draft.save()
    }
    function cancel() {
        // The word being composed goes with the rest.
        Qt.inputMethod.reset()
        Qt.inputMethod.hide()
        root.avatarTooBig = ""
        draft.reset()
    }
    // Scrolls `item` (a field) into the part of the page the keyboard and
    // the bar leave.
    function reveal(item) {
        if (!item || !item.visible)
            return
        const at = item.mapToItem(scroll.contentItem, 0, 0)
        const room = scroll.height - root.bottomSpace
        const top = at.y - 16
        const bottom = at.y + item.height + 16
        if (bottom > scroll.contentY + room)
            scroll.contentY = Math.max(0, Math.min(bottom - room, scroll.contentHeight - scroll.height))
        else if (top < scroll.contentY)
            scroll.contentY = Math.max(0, top)
    }

    PageHeader {
        id: header
        z: 1
        title: qsTr("Edit profile")
        topInset: root.topInset
        leftInset: root.leftInset
        rightInset: root.rightInset
        onBackRequested: root.requestBack()
        Row {
            visible: draft.saved && !draft.dirty
            height: 48
            rightPadding: 8
            spacing: 6
            Accessible.role: Accessible.StaticText
            Accessible.name: qsTr("Changes saved")
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: "check"
                size: Theme.iconSm
                color: Theme.successInk
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Saved")
                color: Theme.ink2
                font: Theme.type.description
            }
        }
    }

    Flickable {
        id: scroll
        anchors.top: header.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        contentHeight: body.height + root.bottomSpace
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickDeceleration: Native.flingDeceleration
        maximumFlickVelocity: Native.maxFlingVelocity

        Column {
            id: body
            x: Math.max(16 + root.leftInset, (scroll.width - width) / 2)
            width: Math.min(scroll.width - 32 - root.leftInset - root.rightInset, 560)
            topPadding: 20
            bottomPadding: 32
            spacing: 32

            // The draft as others see it.
            Item {
                id: preview
                width: parent.width
                height: banner.height + 52 + names.height
                Accessible.role: Accessible.Graphic
                Accessible.name: qsTr("Profile preview")
                Rectangle {
                    id: banner
                    width: parent.width
                    height: Math.round(Math.min(140, width * 0.36))
                    radius: Theme.radius
                    color: draft.bannerColor.length > 0 ? draft.bannerColor
                        : draft.accentColor.length > 0 ? draft.accentColor : Theme.accentTint
                    clip: true
                    Image {
                        anchors.fill: parent
                        visible: draft.shownBanner.length > 0
                        source: draft.shownBanner
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                        sourceSize.width: 720
                    }
                    TapHandler {
                        onTapped: root.choose("banner")
                    }
                }
                Rectangle {
                    id: face
                    x: 12
                    y: banner.height - 44
                    width: 96
                    height: 96
                    radius: Session.meDeco.length > 0 && draft.shownAvatar.length > 0 ? 48 : Theme.radius + 4
                    color: root.color
                    Avatar {
                        anchors.centerIn: parent
                        size: 88
                        initials: Settings.initials.length > 0 ? Settings.initials : "?"
                        source: draft.shownAvatar
                        crop: draft.shownCrop
                        deco: draft.shownAvatar.length > 0 ? Session.meDeco : ""
                    }
                    TapHandler {
                        onTapped: root.choose("avatar")
                    }
                }
                Column {
                    id: names
                    y: banner.height + 52
                    width: parent.width
                    spacing: 2
                    DisplayName {
                        width: parent.width
                        text: draft.displayName.trim().length > 0 ? draft.displayName.trim() : Settings.username
                        nameStyle: draft.nameStyle
                        color: Theme.ink1
                        font: Theme.type.pageTitle
                        ground: root.color
                    }
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: "@" + Settings.username + (draft.pronouns.trim().length > 0 ? " · " + draft.pronouns.trim() : "")
                        elide: Text.ElideRight
                        color: Theme.ink2
                        font: Theme.type.description
                    }
                    Text {
                        width: parent.width
                        visible: draft.customStatus.trim().length > 0
                        topPadding: 6
                        textFormat: Text.PlainText
                        text: draft.customStatus.trim()
                        wrapMode: Text.Wrap
                        color: Theme.ink1
                        font: Theme.type.description
                    }
                }
            }

            Section {
                number: 1
                title: qsTr("Avatar")
                Note {
                    text: draft.avatarNote
                }
                Flow {
                    width: parent.width
                    spacing: 8
                    KitButton {
                        iconName: "image"
                        text: qsTr("Choose photo")
                        Accessible.name: qsTr("Choose an avatar from your photos")
                        enabled: !draft.saving
                        onClicked: root.choose("avatar")
                    }
                    KitButton {
                        variant: "secondary"
                        text: qsTr("Adjust")
                        Accessible.name: qsTr("Frame your avatar")
                        enabled: draft.shownAvatar.length > 0 && !draft.saving
                        onClicked: framer.open("", draft.shownAvatar, draft.shownCrop)
                    }
                    KitButton {
                        visible: draft.canRemoveAvatar
                        variant: "ghost"
                        text: qsTr("Remove")
                        Accessible.name: qsTr("Remove avatar")
                        enabled: !draft.saving
                        onClicked: {
                            root.avatarTooBig = ""
                            draft.dropAvatar()
                        }
                    }
                }
                TooBig {
                    info: root.avatarTooBig
                    onChooseAnother: root.choose("avatar")
                }
            }

            Section {
                number: 2
                title: qsTr("Banner")
                Note {
                    text: draft.bannerNote
                }
                Flow {
                    width: parent.width
                    spacing: 8
                    KitButton {
                        iconName: "image"
                        text: qsTr("Choose image")
                        Accessible.name: qsTr("Choose a banner from your photos")
                        enabled: !draft.saving
                        onClicked: root.choose("banner")
                    }
                    KitButton {
                        visible: draft.shownBanner.length > 0
                        variant: "ghost"
                        text: qsTr("Remove")
                        Accessible.name: qsTr("Remove banner")
                        enabled: !draft.saving
                        onClicked: draft.dropBanner()
                    }
                }
                TooBig {
                    info: draft.bannerTooBig
                    onChooseAnother: root.choose("banner")
                }
            }

            Section {
                number: 3
                title: qsTr("About you")
                Field {
                    label: qsTr("Display name")
                    // Not clamped: the core's check says what is wrong.
                    clamp: false
                    max: draft.limits.displayName
                    placeholder: Settings.username
                    value: draft.displayName
                    error: draft.nameError
                    onEdited: value => draft.displayName = value
                }
                Field {
                    label: qsTr("Pronouns")
                    max: draft.limits.pronouns
                    placeholder: qsTr("they/them")
                    value: draft.pronouns
                    onEdited: value => draft.pronouns = value
                }
                Field {
                    label: qsTr("Custom status")
                    max: draft.limits.customStatus
                    placeholder: qsTr("Shipping the release")
                    value: draft.customStatus
                    onEdited: value => draft.customStatus = value
                }
                Field {
                    label: qsTr("About me")
                    max: draft.limits.bio
                    multiline: true
                    placeholder: qsTr("Night owl, synth collector, always up for co-op.")
                    value: draft.bio
                    onEdited: value => draft.bio = value
                }
            }
        }
    }

    // Cancel and Save over the keyboard (the desktop's unsaved bar), with
    // what went wrong above them.
    Rectangle {
        id: bar
        z: 2
        y: root.height - root.keyboardPad - (root.barShown ? height : 0)
        width: parent.width
        height: barColumn.height + 24
        visible: root.barShown || y < root.height - root.keyboardPad
        color: Theme.ply1
        Behavior on y { NumberAnimation { duration: Theme.dur3; easing.type: Easing.OutCubic } }
        Rectangle {
            width: parent.width
            height: 1
            color: Theme.line
        }
        Column {
            id: barColumn
            x: 16 + root.leftInset
            y: 12
            width: parent.width - 32 - root.leftInset - root.rightInset
            spacing: 10
            Text {
                width: parent.width
                visible: draft.error.length > 0
                textFormat: Text.PlainText
                text: draft.error
                wrapMode: Text.Wrap
                color: Theme.dangerInk
                font: Theme.type.description
                Accessible.role: Accessible.AlertMessage
                Accessible.name: text
            }
            Row {
                width: parent.width
                spacing: 8
                KitButton {
                    width: (parent.width - parent.spacing) / 2
                    size: "lg"
                    variant: "secondary"
                    text: qsTr("Cancel")
                    Accessible.name: qsTr("Discard changes")
                    enabled: !draft.saving
                    onClicked: root.cancel()
                }
                KitButton {
                    width: (parent.width - parent.spacing) / 2
                    size: "lg"
                    variant: "primary"
                    ground: Theme.ply1
                    text: qsTr("Save")
                    Accessible.name: qsTr("Save changes")
                    loading: draft.saving
                    enabled: !draft.saving && draft.nameError.length === 0
                    onClicked: {
                        Native.haptic(Native.Confirm)
                        root.save()
                    }
                }
            }
        }
    }

    ConfirmSheet {
        id: discardSheet
        anchors.fill: parent
        title: qsTr("Discard changes?")
        body: qsTr("Your profile edits have not been saved.")
        confirmText: qsTr("Discard")
        danger: true
        onConfirmed: root.closeRequested()
    }

    AvatarFramePage {
        id: framer
        anchors.fill: parent
        name: Settings.name
        round: Session.meDeco.length > 0
        onApplied: (filePath, crop) => {
            root.avatarTooBig = ""
            draft.localError = ""
            draft.frameAvatar(filePath, crop)
        }
    }

    // A numbered section (Settings' rule card): a 2 px ink-1 rule, the
    // silkscreen legend and its "01", then the rows.
    component Section: Column {
        id: section
        property int number: 0
        property string title
        default property alias content: rows.data
        width: parent ? parent.width : 0
        spacing: 0
        Rectangle {
            width: parent.width
            height: 2
            color: Theme.ink1
        }
        Item {
            width: parent.width
            height: 36
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: section.title
                color: Theme.ink2
                font: Theme.type.silk
                Accessible.role: Accessible.Heading
                Accessible.name: section.title
            }
            Text {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                text: String(section.number).padStart(2, "0")
                color: Theme.ink3
                font: Theme.type.numSm
                Accessible.ignored: true
            }
        }
        Column {
            id: rows
            width: parent.width
            spacing: 12
        }
    }

    component Note: Text {
        width: parent ? parent.width : 0
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: Theme.ink2
        font: Theme.type.description
    }

    // A picture over its limit (the core's too-big card): what and why,
    // and Choose another.
    component TooBig: Rectangle {
        id: tooBig
        property string info
        readonly property var card: info.length > 0 ? JSON.parse(info) : ({})
        signal chooseAnother()
        visible: info.length > 0
        width: parent ? parent.width : 0
        height: visible ? tooBigText.height + 24 : 0
        radius: Theme.radius
        color: Theme.ply2
        border.width: 1
        border.color: Theme.mix(Theme.warningInk, 0.36)
        Accessible.role: Accessible.AlertMessage
        Accessible.name: card.alert ?? ""
        Column {
            id: tooBigText
            x: 12
            y: 12
            width: parent.width - 24
            spacing: 4
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: tooBig.card.headline ?? ""
                wrapMode: Text.Wrap
                color: Theme.ink1
                font: Theme.type.labelStrong
            }
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: tooBig.card.line ?? ""
                wrapMode: Text.Wrap
                color: Theme.ink2
                font: Theme.type.caption
            }
            Item { width: 1; height: 4 }
            KitButton {
                variant: "secondary"
                ground: Theme.ply2
                text: qsTr("Choose another")
                onClicked: tooBig.chooseAnother()
            }
        }
    }

    // A labelled field with its count ("12/40") and, when the core refuses
    // it, why. One line, or (multiline) a box that grows with the text.
    component Field: Column {
        id: field
        property string label
        property string value
        property string placeholder
        property string error
        property int max: 40
        // Clamped at `max` while typing; else the count turns red past it.
        property bool clamp: true
        property bool multiline: false
        // `value` is what the field shows, the word the keyboard is still
        // composing included (ThreadPage's send key): the draft, its count,
        // Save and the Back guard see it before the keyboard commits it.
        // Lengths are UTF-16 units, as the server and the core count.
        readonly property bool over: value.length > max
        signal edited(string value)
        width: parent ? parent.width : 0
        spacing: 6
        function focused(on) {
            if (on) {
                root.typing = field
                Qt.callLater(root.reveal, field)
            } else if (root.typing === field) {
                root.typing = null
            }
        }
        Text {
            textFormat: Text.PlainText
            text: field.label
            color: Theme.ink1
            font: Theme.type.labelStrong
        }
        MobileField {
            id: line
            visible: !field.multiline
            width: parent.width
            maximumLength: field.clamp ? field.max : 32767
            placeholderText: field.placeholder
            invalid: field.error.length > 0 || field.over
            inputMethodHints: Qt.ImhNoAutoUppercase
            EnterKey.type: Qt.EnterKeyDone
            Accessible.name: field.label
            readonly property string live: text + preeditText
            // Follows the draft without a binding: the draft holds the word
            // being composed too, which `text` must not take.
            Component.onCompleted: text = field.value
            Connections {
                target: field
                function onValueChanged() {
                    if (line.live !== field.value)
                        line.text = field.value
                }
            }
            // Only the shown editor edits (the other just follows).
            onLiveChanged: if (!field.multiline && live !== field.value) field.edited(live)
            onActiveFocusChanged: field.focused(activeFocus)
            onAccepted: {
                focus = false
                Qt.inputMethod.hide()
            }
        }
        TextArea {
            id: area
            visible: field.multiline
            width: parent.width
            height: visible ? Math.max(112, implicitHeight) : 0
            leftPadding: 14
            rightPadding: 14
            topPadding: 12
            bottomPadding: 12
            wrapMode: TextEdit.Wrap
            color: Theme.ink1
            placeholderText: field.placeholder
            placeholderTextColor: Theme.ink3
            selectionColor: Theme.selection
            selectedTextColor: Theme.ink1
            font: Theme.type.message
            Accessible.name: field.label
            background: Rectangle {
                radius: Theme.radius
                color: Theme.ply2
                border.width: area.activeFocus || field.over ? 2 : 1
                border.color: field.over ? Theme.dangerInk : area.activeFocus ? Theme.ink1 : Theme.lineField
                Behavior on border.color { ColorAnimation { duration: Theme.dur1 } }
            }
            readonly property string live: text + preeditText
            // Follows the draft without a binding: the clamp assigns `text`,
            // and the draft holds the word being composed too.
            Component.onCompleted: text = field.value
            Connections {
                target: field
                function onValueChanged() {
                    if (area.live !== field.value)
                        area.text = field.value
                }
            }
            onTextChanged: {
                if (field.multiline && field.clamp && text.length > field.max) {
                    // Not between the halves of a surrogate pair (an emoji).
                    let cut = field.max
                    const unit = text.charCodeAt(cut - 1)
                    if (unit >= 0xD800 && unit <= 0xDBFF)
                        cut -= 1
                    const at = cursorPosition
                    text = text.slice(0, cut)
                    cursorPosition = Math.min(at, cut)
                }
            }
            onLiveChanged: if (field.multiline && live !== field.value) field.edited(live)
            onActiveFocusChanged: field.focused(activeFocus)
            onHeightChanged: if (activeFocus) Qt.callLater(root.reveal, field)
        }
        Item {
            width: parent.width
            height: Math.max(errorText.height, count.height)
            Text {
                id: errorText
                width: parent.width - count.width - 12
                visible: field.error.length > 0
                textFormat: Text.PlainText
                text: field.error
                wrapMode: Text.Wrap
                color: Theme.dangerInk
                font: Theme.type.caption
                Accessible.role: Accessible.AlertMessage
                Accessible.name: text
            }
            Text {
                id: count
                anchors.right: parent.right
                textFormat: Text.PlainText
                text: field.value.length + "/" + field.max
                color: field.over ? Theme.dangerInk : Theme.ink3
                font: Theme.type.numSm
            }
        }
    }
}
