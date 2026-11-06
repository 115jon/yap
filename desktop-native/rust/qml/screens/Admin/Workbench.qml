pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import com.yap.native

// The settings workbench every settings-like surface shares
// (SettingsWorkbench.tsx), here for server and channel settings: a
// min(1120, window - 32) x min(820, window - 32) dialog over the scrim. The
// 264 px ply-1 nav: the title (Bricolage 17/800) over the subtitle and the
// Esc cap in its 48 px header, then one untitled group of 36 px rows. The
// page: a 48 px header (an optional back key, the page title, the 40 px
// close key) over one scroller, padded 32, content capped at 760 with 40
// between sections unless `wide`, then the unsaved bar.
// Escape steps back: an inline confirm, then the back key, then closes;
// with unsaved changes, closing or switching page nudges the bar instead.
T.Popup {
    id: bench

    property string title
    property string subtitle
    // [{id, label, icon}]
    property var items: []
    property string activeId
    property string pageTitle
    // Non-empty: a nested page; the header shows a back key with this name.
    property string backLabel: ""
    // The page lays itself out (split editors) instead of the 760 column.
    property bool wide: false
    // The unsaved bar (UnsavedBar.tsx), fed by the page.
    property bool dirty: false
    property bool saving: false
    property string saveError
    property string saveLabel: qsTr("Save changes")
    property bool saveDisabled: false
    default property alias content: column.data

    signal selected(string id)
    signal backRequested()
    signal saveRequested()
    signal resetRequested()

    // Inline confirms take Escape first: each is an item with a `dismiss()`.
    property var escapes: []
    function pushEscape(item) { escapes = escapes.filter(e => e !== item).concat([item]) }
    function popEscape(item) { escapes = escapes.filter(e => e !== item) }

    function nudge() { bar.nudge() }
    function requestClose() {
        if (dirty || saving)
            nudge()
        else
            close()
    }
    function stepBack() {
        const handler = escapes[escapes.length - 1]
        if (handler) {
            handler.dismiss()
            return
        }
        if (backLabel.length > 0) {
            if (dirty)
                nudge()
            else
                backRequested()
            return
        }
        requestClose()
    }
    function choose(id) {
        if (dirty && id !== activeId)
            nudge()
        else
            selected(id)
    }
    function toTop() { body.contentY = 0 }
    // The page is scrolled to within 480 px of its end: long lists
    // (PagedRows) build their next rows.
    readonly property bool nearEnd: body.contentHeight - body.contentY - body.height < 480

    parent: T.Overlay.overlay
    // Centred under the title bar, which the web keeps above overlays.
    x: Math.round((parent.width - width) / 2)
    y: Theme.chrome + Math.round((parent.height - Theme.chrome - height) / 2)
    width: Math.min(1120, parent.width - 32)
    height: Math.min(820, parent.height - Theme.chrome - 32)
    padding: 0
    modal: true
    // Ctrl+K stays shut while a modal is up (ExtrasHost).
    onVisibleChanged: ShellState.modalCount += visible ? 1 : -1
    focus: true
    closePolicy: T.Popup.NoAutoClose
    onOpened: navList.forceActiveFocus()
    onClosed: escapes = []
    onActiveIdChanged: toTop()

    T.Overlay.modal: Scrim {
        color: Theme.scrim
        TapHandler { onTapped: bench.requestClose() }
    }

    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.dur4; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
            NumberAnimation { property: "scale"; from: Theme.reducedMotion ? 1 : 0.98; to: 1; duration: Theme.dur4; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
        }
    }
    exit: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; to: 0; duration: Theme.dur4Exit }
            NumberAnimation { property: "scale"; to: Theme.reducedMotion ? 1 : 0.98; duration: Theme.dur4Exit }
        }
    }

    background: Rectangle {
        radius: Theme.radiusSheet
        color: Theme.ply0
        border.width: 1
        border.color: Theme.line
        FloatShadow { modal: true; radius: Theme.radiusSheet }
    }

    contentItem: FocusScope {
        Keys.onEscapePressed: event => {
            // Escape in a text field belongs to the field, not the workbench.
            const focused = Window.activeFocusItem
            if (focused !== null && "cursorPosition" in focused) {
                event.accepted = false
                return
            }
            event.accepted = true
            bench.stepBack()
        }

        // ---------- Nav ----------
        Rectangle {
            id: navColumn
            width: 264
            height: parent.height
            radius: Theme.radiusSheet
            color: Theme.ply1
            Accessible.role: Accessible.Grouping
            Accessible.name: qsTr("%1 navigation").arg(bench.title)
            Rectangle {
                anchors.right: parent.right
                width: Theme.radiusSheet
                height: parent.height
                color: Theme.ply1
            }
            Rectangle {
                anchors.right: parent.right
                width: 1
                height: parent.height
                color: Theme.line
            }

            Item {
                id: navHeader
                width: parent.width
                height: Theme.header
                Column {
                    anchors.left: parent.left
                    anchors.leftMargin: 16
                    anchors.right: escCap.left
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: bench.title
                        elide: Text.ElideRight
                        color: Theme.ink1
                        font: Theme.type.serverTitle
                        Accessible.role: Accessible.Heading
                    }
                    Text {
                        visible: bench.subtitle.length > 0
                        width: parent.width
                        textFormat: Text.PlainText
                        text: bench.subtitle
                        elide: Text.ElideRight
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                }
                KeyCap {
                    id: escCap
                    anchors.right: parent.right
                    anchors.rightMargin: 16
                    anchors.verticalCenter: parent.verticalCenter
                    outline: true
                    text: qsTr("Esc")
                }
                Rectangle {
                    anchors.bottom: parent.bottom
                    width: parent.width - 1
                    height: 1
                    color: Theme.line
                }
            }

            FocusScope {
                id: navList
                anchors.top: navHeader.bottom
                anchors.topMargin: 24
                x: 8
                width: parent.width - 17
                height: navRows.implicitHeight
                activeFocusOnTab: true
                Accessible.role: Accessible.List
                onActiveFocusChanged: {
                    if (!activeFocus)
                        return
                    for (const row of navRows.children) {
                        if (row.itemId === bench.activeId) {
                            row.forceActiveFocus()
                            return
                        }
                    }
                }
                Column {
                    id: navRows
                    width: parent.width
                    spacing: 2
                    Repeater {
                        model: bench.items
                        delegate: T.AbstractButton {
                            id: navRow
                            required property var modelData
                            readonly property string itemId: modelData.id
                            readonly property bool current: bench.activeId === itemId
                            width: navRows.width
                            height: 36
                            focusPolicy: Qt.StrongFocus
                            Accessible.role: Accessible.PageTab
                            Accessible.name: modelData.label
                            Accessible.selected: current
                            onClicked: bench.choose(itemId)
                            Keys.onUpPressed: nextItemInFocusChain(false).forceActiveFocus()
                            Keys.onDownPressed: nextItemInFocusChain(true).forceActiveFocus()
                            background: Rectangle {
                                radius: Theme.radius
                                color: navRow.current ? Theme.tint2 : navRow.hovered ? Theme.tint1 : "transparent"
                                border.width: navRow.visualFocus ? 2 : 0
                                border.color: Theme.focus
                                Behavior on color { ColorAnimation { duration: Theme.dur1 } }
                            }
                            contentItem: Item {
                                Icon {
                                    id: glyph
                                    x: 8
                                    anchors.verticalCenter: parent.verticalCenter
                                    name: navRow.modelData.icon
                                    variant: navRow.current ? "active" : "idle"
                                    size: Theme.iconLg
                                    color: navRow.current ? Theme.ink1 : Theme.ink3
                                }
                                Text {
                                    anchors.left: glyph.right
                                    anchors.leftMargin: 12
                                    anchors.right: parent.right
                                    anchors.rightMargin: 8
                                    anchors.verticalCenter: parent.verticalCenter
                                    textFormat: Text.PlainText
                                    text: navRow.modelData.label
                                    elide: Text.ElideRight
                                    color: navRow.current || navRow.hovered ? Theme.ink1 : Theme.ink2
                                    font: navRow.current ? Theme.type.rowUnread : Theme.type.row
                                }
                            }
                        }
                    }
                }
            }
        }

        // ---------- Page ----------
        Item {
            id: page
            anchors.left: navColumn.right
            anchors.right: parent.right
            height: parent.height
            Accessible.role: Accessible.Pane
            Accessible.name: qsTr("%1 content").arg(bench.title)

            Item {
                id: pageHeader
                width: parent.width
                height: Theme.header
                IconKey {
                    id: backKey
                    visible: bench.backLabel.length > 0
                    x: 12
                    anchors.verticalCenter: parent.verticalCenter
                    iconName: "chevronLeft"
                    tip: bench.backLabel
                    tipSide: "bottom"
                    onClicked: bench.stepBack()
                }
                Text {
                    anchors.left: backKey.visible ? backKey.right : parent.left
                    anchors.leftMargin: backKey.visible ? 8 : 16
                    anchors.right: closeKey.left
                    anchors.rightMargin: 12
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: bench.pageTitle
                    elide: Text.ElideRight
                    color: Theme.ink1
                    font: Theme.type.columnTitle
                    Accessible.role: Accessible.Heading
                }
                IconKey {
                    id: closeKey
                    anchors.right: parent.right
                    anchors.rightMargin: 16
                    anchors.verticalCenter: parent.verticalCenter
                    iconName: "close"
                    tone: "toggled"
                    tip: qsTr("Close %1").arg(bench.title.toLowerCase())
                    tipSide: "bottom"
                    onClicked: bench.requestClose()
                }
                Rectangle {
                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: 1
                    color: Theme.line
                }
            }

            Flickable {
                id: body
                anchors.top: pageHeader.bottom
                anchors.bottom: bar.top
                width: parent.width
                clip: true
                contentHeight: column.height + 32 + 32
                boundsBehavior: Flickable.StopAtBounds
                T.ScrollBar.vertical: ThinScrollBar {}

                Column {
                    id: column
                    x: 32
                    y: 32
                    width: bench.wide ? body.width - 64 : Math.min(body.width - 64, 760)
                    spacing: 40
                }
            }

            UnsavedBar {
                id: bar
                anchors.bottom: parent.bottom
                width: parent.width
                dirty: bench.dirty
                saving: bench.saving
                error: bench.saveError
                saveLabel: bench.saveLabel
                saveDisabled: bench.saveDisabled
                onReset: bench.resetRequested()
                onSave: bench.saveRequested()
            }
        }
    }
}
