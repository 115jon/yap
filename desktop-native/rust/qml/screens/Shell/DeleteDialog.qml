import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// DeleteMessageDialog.tsx: title, consequence, the message itself, the
// Shift hint, then Cancel and Delete. Delete has focus; Esc cancels.
Popup {
    id: root

    property string messageId
    property string author
    property string text
    // The preview's extras (MessagePreviewCard): avatar, initials, time,
    // edited, attachments (the row's media items), or null.
    property var info: null
    readonly property var media: info && info.attachments
        ? info.attachments.filter(a => a.kind === "image" || a.kind === "video") : []
    readonly property int files: info && info.attachments ? info.attachments.length - media.length : 0
    signal confirmed(string id)

    function ask(id, author, text, extra) {
        root.messageId = id
        root.author = author
        root.text = text
        root.info = extra ?? null
        open()
    }

    parent: Overlay.overlay
    anchors.centerIn: parent
    width: 400
    modal: true
    // Ctrl+K stays shut while a modal is up (ExtrasHost).
    onVisibleChanged: ShellState.modalCount += visible ? 1 : -1
    focus: true
    padding: 24
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onOpened: deleteKey.forceActiveFocus()

    Overlay.modal: Scrim { color: Theme.scrim }

    background: Rectangle {
        radius: Theme.radiusSheet
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        FloatShadow { modal: true; radius: Theme.radiusSheet }
    }

    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
            NumberAnimation { property: "scale"; from: Theme.scaleDialog; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
        }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: Theme.dur3Exit }
    }

    // ResponsiveDialogContent sm: the title at Bricolage 22/700 beside a
    // 40 px close key, the consequence at 15 in ink-2, the message as a
    // MessagePreviewCard (tint-1, p-3, a 32 avatar, 13/600 name over 13
    // ink-2), the Shift hint with its key cap, then Cancel and Delete (48).
    contentItem: Column {
        spacing: 16

        Item {
            width: parent.width
            height: titleText.implicitHeight - 8
            Text {
                id: titleText
                anchors.left: parent.left
                anchors.right: closeKey.left
                anchors.rightMargin: 12
                anchors.top: parent.top
                text: qsTr("Delete message")
                wrapMode: Text.Wrap
                color: Theme.ink1
                font.family: "Yap Bricolage Head"
                font.pixelSize: 22
                font.weight: Font.Bold
                font.letterSpacing: -0.22
                lineHeight: 28
                lineHeightMode: Text.FixedHeight
                Accessible.role: Accessible.Heading
            }
            IconKey {
                id: closeKey
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.topMargin: -8
                anchors.rightMargin: -8
                iconName: "close"
                tone: "rest"
                tip: qsTr("Close")
                onClicked: root.close()
            }
        }
        Text {
            // The header's gap-2 under the title, not the body's 16.
            width: parent.width
            text: qsTr("This deletes the message for everyone. You can't undo it.")
            wrapMode: Text.Wrap
            color: Theme.ink2
            font: Theme.type.message
            lineHeight: Theme.type.messageLine
            lineHeightMode: Text.FixedHeight
        }
        // The message (MessagePreviewCard.tsx), inert: tint-1, p-3, gap-3;
        // the 32 avatar, the name at 13/600 with the time in Num ink-3 on
        // its baseline, the body at 13/20 ink-2 (4 lines) with "(edited)",
        // then up to four 40 px media cells (a picture, or the play glyph
        // for a video) and a count of the rest and of files.
        Rectangle {
            width: parent.width
            height: Math.max(32, quote.implicitHeight) + 24
            radius: Theme.radius
            color: Theme.tint1
            Avatar {
                x: 12
                y: 12
                size: 32
                initials: root.info ? root.info.initials : root.author
                source: root.info ? root.info.avatar : ""
            }
            Column {
                id: quote
                x: 12 + 32 + 12
                y: 12
                width: parent.width - x - 12
                spacing: 2
                Row {
                    width: parent.width
                    spacing: 8
                    Text {
                        id: authorLine
                        width: Math.min(implicitWidth, parent.width - stamp.implicitWidth - 8)
                        textFormat: Text.PlainText
                        text: root.author
                        elide: Text.ElideRight
                        color: Theme.ink1
                        font: Theme.type.labelStrong
                        lineHeight: 18
                        lineHeightMode: Text.FixedHeight
                    }
                    Text {
                        id: stamp
                        anchors.baseline: authorLine.baseline
                        textFormat: Text.PlainText
                        text: root.info ? root.info.time : ""
                        color: Theme.ink3
                        font: Theme.type.numSm
                    }
                }
                Text {
                    width: parent.width
                    visible: root.text.trim().length > 0
                    text: root.text + (root.info && root.info.edited ? " (edited)" : "")
                    wrapMode: Text.Wrap
                    maximumLineCount: 4
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                    color: Theme.ink2
                    font: Theme.type.description
                    lineHeight: 20
                    lineHeightMode: Text.FixedHeight
                }
                Item { width: 1; height: 6; visible: cells.visible }
                Flow {
                    id: cells
                    visible: root.media.length > 0 || root.files > 0
                    width: parent.width
                    spacing: 4
                    Repeater {
                        model: root.media.slice(0, 4)
                        delegate: Rectangle {
                            required property var modelData
                            width: 40
                            height: 40
                            radius: Theme.radius
                            color: Theme.tint2
                            clip: true
                            Icon {
                                anchors.centerIn: parent
                                visible: parent.modelData.kind === "video"
                                name: "play"
                                variant: "active"
                                size: Theme.iconSm
                                color: Theme.ink2
                            }
                            Image {
                                anchors.fill: parent
                                visible: parent.modelData.kind !== "video"
                                source: parent.modelData.kind !== "video" ? parent.modelData.url : ""
                                // A sensitive picture decodes tiny: the blur.
                                sourceSize: parent.modelData.nsfw === true && ShellState.mediaFilter === "high" ? Qt.size(8, 8) : Qt.size(80, 80)
                                fillMode: Image.PreserveAspectCrop
                                asynchronous: true
                            }
                        }
                    }
                    Row {
                        visible: root.media.length > 4 || root.files > 0
                        height: 40
                        leftPadding: 4
                        spacing: 4
                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: "paperclip"
                            size: Theme.iconXs
                            color: Theme.ink3
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: (root.media.length > 4 ? "+" + (root.media.length - 4) + " " : "")
                                + (root.files > 0 ? root.files + (root.files === 1 ? qsTr(" file") : qsTr(" files")) : "")
                            color: Theme.ink3
                            font: Theme.type.caption
                        }
                    }
                }
            }
        }
        Row {
            width: parent.width
            spacing: 4
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Hold")
                color: Theme.ink3
                font: Theme.type.caption
            }
            KeyCap {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Shift")
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("when you click Delete to skip this.")
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
        Row {
            anchors.right: parent.right
            topPadding: 8
            spacing: 8
            DialogKey {
                text: qsTr("Cancel")
                onClicked: root.close()
            }
            DialogKey {
                id: deleteKey
                text: qsTr("Delete")
                danger: true
                onClicked: {
                    root.confirmed(root.messageId)
                    root.close()
                }
            }
        }
    }

    component DialogKey: Button {
        id: key
        property bool danger: false
        implicitHeight: Theme.controlLg
        leftPadding: 20
        rightPadding: 20
        font: Theme.type.control
        Keys.onReturnPressed: key.clicked()
        contentItem: Text {
            textFormat: Text.PlainText
            text: key.text
            font: key.font
            color: key.danger ? Theme.dangerFillInk : Theme.ink1
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle {
            radius: Theme.radius
            color: key.danger ? Theme.dangerFill : "transparent"
            Rectangle {
                anchors.fill: parent
                radius: Theme.radius
                color: key.danger ? Theme.dangerFillInk : Theme.ink1
                opacity: key.down ? 0.14 : key.hovered ? 0.08 : 0
            }
            Rectangle {
                anchors.fill: parent
                anchors.margins: -3
                visible: key.visualFocus
                color: "transparent"
                radius: Theme.radius
                border.width: 2
                border.color: Theme.focus
            }
        }
        HoverHandler { cursorShape: Qt.PointingHandCursor }
    }
}
