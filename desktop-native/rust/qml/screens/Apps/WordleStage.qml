pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// Daily Word (WordleActivityStage.tsx) in place of the call's tiles: the
// board, the keyboard, hints, hard mode and the other players' marks.
// Letters typed while the stage has focus go to the board.
FocusScope {
    id: root

    // The state is parsed afresh on every change, and a new object would
    // rebuild every Repeater under it; this slice is replaced only when its
    // content differs (as FriendsView and SearchPanel do).
    readonly property var parsed: JSON.parse(Apps.state || "{}")
    property var w: ({})
    property string wKey: "{}"

    function syncW() {
        const next = parsed.wordle ?? {}
        const key = JSON.stringify(next)
        if (key !== wKey) {
            wKey = key
            w = next
        }
    }
    onParsedChanged: syncW()
    property bool confirmReveal: false

    function markColor(mark) {
        return mark === "correct" ? Theme.talk : mark === "present" ? Theme.idle
            : mark === "absent" ? Theme.tint3 : "transparent"
    }

    Component.onCompleted: {
        syncW()
        Apps.wordleOpen()
        forceActiveFocus()
    }
    Timer {
        interval: 60000
        repeat: true
        running: root.visible
        onTriggered: Apps.wordleOpen()
    }
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
            Apps.wordleKey("enter")
        else if (event.key === Qt.Key_Backspace)
            Apps.wordleKey("backspace")
        else if (/^[a-zA-Z]$/.test(event.text))
            Apps.wordleKey(event.text.toLowerCase())
        else
            return
        event.accepted = true
    }
    MouseArea { anchors.fill: parent; onPressed: mouse => { root.forceActiveFocus(); mouse.accepted = false } }

    WordleHeader {
        id: header
        width: parent.width
        wordle: root.w
        confirmReveal: root.confirmReveal
        onRevealAsked: root.confirmReveal = true
        onRevealCancelled: root.confirmReveal = false
        onRevealConfirmed: {
            Apps.wordleReveal()
            root.confirmReveal = false
        }
    }

    Column {
        id: game
        anchors.top: header.bottom
        anchors.topMargin: 12
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.horizontalCenterOffset: players.visible ? -110 : 0
        spacing: 6

        Text {
            visible: root.w.loading === true || (root.w.error ?? "").length > 0
            anchors.horizontalCenter: parent.horizontalCenter
            textFormat: Text.PlainText
            text: root.w.loading ? qsTr("Loading today's puzzle…") : root.w.error ?? ""
            color: root.w.loading ? Theme.ink3 : Theme.dangerInk
            font: Theme.type.description
        }
        KitButton {
            visible: !root.w.loading && (root.w.error ?? "").length > 0
            anchors.horizontalCenter: parent.horizontalCenter
            size: "sm"
            text: qsTr("Try again")
            onClicked: Apps.wordleOpen()
        }
        Column {
            id: board
            spacing: 6
            transform: Translate { id: nudge }
            Repeater {
                model: root.w.board ?? []
                delegate: Row {
                    id: boardRow
                    required property var modelData
                    spacing: 6
                    Repeater {
                        model: boardRow.modelData
                        delegate: Rectangle {
                            id: cell
                            required property var modelData
                            readonly property bool open: modelData.mark === "typed" || modelData.mark === ""
                            width: 52
                            height: 52
                            radius: 4
                            color: root.markColor(modelData.mark)
                            border.width: open ? 2 : 0
                            border.color: modelData.mark === "typed" ? Theme.ink2 : Theme.line
                            Text {
                                anchors.centerIn: parent
                                textFormat: Text.PlainText
                                text: cell.modelData.letter
                                color: cell.modelData.mark === "correct" ? Theme.talkFillInk : Theme.ink1
                                font: Theme.type.sectionTitle
                            }
                        }
                    }
                }
            }
        }
        // A word off the list shakes the board (`invalidGuessAttempt`).
        SequentialAnimation {
            id: shake
            NumberAnimation { target: nudge; property: "x"; to: -8; duration: 50 }
            NumberAnimation { target: nudge; property: "x"; to: 8; duration: 70 }
            NumberAnimation { target: nudge; property: "x"; to: 0; duration: 50 }
        }
        Connections {
            target: root
            property int seen: 0
            function onWChanged() {
                const invalid = root.w.invalid ?? 0
                if (invalid > seen)
                    shake.restart()
                seen = invalid
            }
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            height: 22
            textFormat: Text.PlainText
            text: root.w.status === "solved" ? qsTr("Solved. Streak %1.").arg(root.w.streak ?? 0)
                : root.w.status === "missed" ? qsTr("The word was %1.").arg(root.w.answer ?? "")
                : root.w.notice ?? ""
            color: root.w.status === "missed" || (root.w.notice ?? "").length > 0 ? Theme.ink1 : Theme.ink2
            font: Theme.type.row
        }
        WordleKeyboard {
            anchors.horizontalCenter: parent.horizontalCenter
            keys: root.w.keys ?? ({})
        }
    }

    WordlePlayers {
        id: players
        visible: (root.w.players ?? []).length > 0
        anchors.top: game.top
        anchors.left: game.right
        anchors.leftMargin: 32
        width: 188
        players: root.w.players ?? []
        markColor: root.markColor
    }
}
