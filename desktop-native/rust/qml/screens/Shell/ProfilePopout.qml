pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Templates as T
import com.yap.native

// The user popout (UserProfilePopover.tsx): the profile card 8 px beside
// what opened it, kept 12 px inside the window. Someone else's card has
// Message (the DM, created when needed) and, when known, About me and
// their roles in the shown server. Opens like a popover: scale 0.97 and a
// fade over 200 ms (exit 140). Esc or a press outside closes it.
T.Popup {
    id: popup

    property Item anchorItem: null
    property string side: "right"
    // Who this card was opened for: closing it closes only them.
    property string shownId

    function openFor(userId, anchor, where) {
        anchorItem = anchor
        side = where
        shownId = userId
        Profile.show(userId)
        open()
        Qt.callLater(place)
    }

    // Beside the anchor on `side`, aligned to its start, inside the window.
    function place() {
        const overlay = T.Overlay.overlay
        if (!anchorItem || !overlay)
            return
        const at = anchorItem.mapToItem(overlay, 0, 0)
        let px = side === "left" ? at.x - width - 8
               : side === "top" ? at.x
               : at.x + anchorItem.width + 8
        let py = side === "top" ? at.y - height - 8 : at.y
        px = Math.max(12, Math.min(px, overlay.width - width - 12))
        py = Math.max(12, Math.min(py, overlay.height - height - 12))
        x = px
        y = py
    }

    parent: T.Overlay.overlay
    width: card.implicitWidth
    height: card.implicitHeight
    padding: 0
    modal: false
    focus: true
    closePolicy: T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutside
    onHeightChanged: if (visible) place()
    onClosed: Profile.close(shownId)

    contentItem: ProfileCard {
        id: card
        name: Profile.name
        username: Profile.username
        avatar: Profile.avatar
        initials: Profile.initials
        pronouns: Profile.pronouns
        presence: Profile.presence
        customStatus: Profile.customStatus
        banner: Profile.banner
        bannerColor: Profile.bannerColor
        Accessible.role: Accessible.Dialog
        Accessible.name: qsTr("User profile for %1").arg(Profile.username)

        // Message: the primary key across the card.
        NotchedKey {
            visible: !Profile.me && Profile.userId.length > 0
            width: parent.width
            large: false
            ground: Theme.ply3
            text: qsTr("Message")
            onClicked: {
                Nav.openDm(Profile.userId)
                popup.close()
            }
        }

        Column {
            visible: Profile.bio.length > 0
            width: parent.width
            spacing: 8
            Text {
                text: qsTr("About me")
                color: Theme.ink2
                font: Theme.type.captionStrong
                Accessible.role: Accessible.Heading
            }
            Text {
                width: parent.width
                text: Profile.bio
                wrapMode: Text.Wrap
                maximumLineCount: 12
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.description
            }
        }

        Column {
            visible: Profile.roleNames.length > 0
            width: parent.width
            spacing: 8
            Text {
                text: qsTr("Roles")
                color: Theme.ink2
                font: Theme.type.captionStrong
                Accessible.role: Accessible.Heading
            }
            Flow {
                width: parent.width
                spacing: 6
                Repeater {
                    model: Profile.roleNames
                    delegate: Rectangle {
                        id: chip
                        required property string modelData
                        required property int index
                        readonly property string tone: Profile.roleColors[index] ?? ""
                        width: Math.min(chipRow.implicitWidth + 8 + 10, parent ? parent.width : 300)
                        height: 24
                        radius: Theme.radius
                        color: Theme.tint1
                        Row {
                            id: chipRow
                            x: 8
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 6
                            Rectangle {
                                anchors.verticalCenter: parent.verticalCenter
                                width: 10
                                height: 10
                                radius: 5
                                color: chip.tone.length > 0 ? chip.tone : Theme.ink3
                            }
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                text: chip.modelData
                                color: Theme.ink1
                                font: Theme.type.captionStrong
                            }
                        }
                    }
                }
            }
        }
    }

    background: null

    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
            NumberAnimation { property: "scale"; from: Theme.scaleIn; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
        }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: Theme.dur3Exit }
    }
}
