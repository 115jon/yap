import QtQuick
import com.yap.native

// The Daily Word bar: title and date, hard mode, hints (unlock the next,
// list the unlocked), Reveal answer with its confirm, and Leave.
Column {
    id: root

    required property var wordle
    property bool confirmReveal: false
    signal revealAsked()
    signal revealCancelled()
    signal revealConfirmed()

    readonly property bool playing: wordle.status === "playing" && !wordle.loading && (wordle.error ?? "").length === 0
    spacing: 8
    topPadding: 12

    Item {
        width: root.width
        height: 40
        Column {
            x: 16
            anchors.verticalCenter: parent.verticalCenter
            Text {
                textFormat: Text.PlainText
                text: qsTr("Daily Word")
                color: Theme.ink1
                font: Theme.type.sectionTitle
            }
            Text {
                textFormat: Text.PlainText
                text: root.wordle.date ?? ""
                color: Theme.ink3
                font: Theme.type.numSm
            }
        }
        Row {
            anchors.right: parent.right
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            SettingSwitch {
                anchors.verticalCenter: parent.verticalCenter
                checked: root.wordle.hardMode === true
                Accessible.name: qsTr("Hard mode")
                onClicked: Apps.wordleHardMode(!checked)
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: qsTr("Hard mode")
                color: Theme.ink2
                font: Theme.type.caption
            }
            KitButton {
                size: "sm"
                variant: "secondary"
                iconName: "lightbulb"
                text: qsTr("Hint")
                enabled: root.playing && (root.wordle.hints ?? []).length < 3
                onClicked: Apps.wordleHint()
            }
            KitButton {
                size: "sm"
                variant: "ghost"
                text: qsTr("Reveal answer")
                enabled: root.playing
                onClicked: root.revealAsked()
            }
            KitButton {
                size: "sm"
                variant: "ghost"
                iconName: "logout"
                text: qsTr("Leave")
                onClicked: Apps.leaveActivity()
            }
        }
    }
    Row {
        x: 16
        visible: root.confirmReveal
        height: visible ? implicitHeight : 0
        spacing: 8
        Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: qsTr("Reveal the answer? Today counts as a miss.")
            color: Theme.ink1
            font: Theme.type.caption
        }
        KitButton { size: "sm"; variant: "ghost"; text: qsTr("Cancel"); onClicked: root.revealCancelled() }
        KitButton { size: "sm"; variant: "destructive"; text: qsTr("Reveal"); onClicked: root.revealConfirmed() }
    }
    Row {
        x: 16
        visible: (root.wordle.hints ?? []).length > 0
        height: visible ? implicitHeight : 0
        spacing: 16
        Repeater {
            model: root.wordle.hints ?? []
            delegate: Text {
                required property var modelData
                textFormat: Text.PlainText
                text: qsTr("%1: %2").arg(modelData.title).arg(modelData.value)
                color: Theme.ink2
                font: Theme.type.captionStrong
            }
        }
    }
}
