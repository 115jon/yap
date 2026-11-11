import QtQuick
import Yap.Mobile
import Yap.Ui

// A message's text on a page of its own, selectable (the menu's "Select
// text"): in the list a press on the text belongs to the row, so the
// platform's selection handles and its copy bar live here. A long press
// selects a word; Copy takes the selection, or all of it when there is
// none. Back, and the predictive back gesture, close it.
FocusScope {
    id: page

    property string text: ""
    property real back: 0
    property real shown: 0

    signal closed()

    function open(body) {
        text = body
        edit.deselect()
        visible = true
        Qt.inputMethod.hide()
        forceActiveFocus()
        Native.pushBackLayer(page)
        fade.to = 1
        fade.restart()
    }
    function close() {
        if (!visible || fade.to === 0)
            return
        Native.popBackLayer(page)
        edit.deselect()
        fade.to = 0
        fade.restart()
    }
    function backProgressed(progress) { back = progress }
    function backCancelled() { back = 0 }
    function backInvoked() { close() }

    visible: false
    z: 110
    opacity: shown * (1 - back * 0.2)
    scale: 1 - back * 0.08
    transform: Translate { y: (1 - page.shown) * Theme.panelShift; x: page.back * page.width * 0.1 }
    Component.onDestruction: Native.popBackLayer(page)

    NumberAnimation {
        id: fade
        target: page
        property: "shown"
        duration: to === 1 ? Theme.dur4 : Theme.dur4Exit
        easing.type: Easing.BezierSpline
        easing.bezierCurve: Theme.easeOut
        onFinished: if (to === 0) {
            page.visible = false
            page.back = 0
            page.closed()
        }
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.ply0
    }
    // Nothing under the page takes a touch.
    MouseArea { anchors.fill: parent }

    Item {
        id: header
        y: page.SafeArea.margins.top
        width: parent.width
        height: 56
        IconKey {
            id: backKey
            x: 8 + page.SafeArea.margins.left
            anchors.verticalCenter: parent.verticalCenter
            iconName: "back"
            tip: qsTr("Back")
            onClicked: page.close()
        }
        Text {
            anchors.left: backKey.right
            anchors.leftMargin: 8
            anchors.right: copyKey.left
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            text: qsTr("Select text")
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: Theme.ink1
            font: Theme.type.columnTitle
        }
        KitButton {
            id: copyKey
            anchors.right: parent.right
            anchors.rightMargin: 12 + page.SafeArea.margins.right
            anchors.verticalCenter: parent.verticalCenter
            size: "sm"
            variant: "secondary"
            iconName: "copy"
            text: edit.selectedText.length > 0 ? qsTr("Copy") : qsTr("Copy all")
            onClicked: {
                Native.copy(edit.selectedText.length > 0 ? edit.selectedText : page.text)
                page.close()
            }
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
    }

    Flickable {
        id: flick
        anchors.top: header.bottom
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: page.SafeArea.margins.left
        anchors.rightMargin: page.SafeArea.margins.right
        contentHeight: edit.implicitHeight + 32 + page.SafeArea.margins.bottom
        clip: true
        boundsBehavior: Flickable.StopAtBounds

        TextEdit {
            id: edit
            x: 16
            y: 16
            width: flick.width - 32
            readOnly: true
            selectByMouse: true
            persistentSelection: true
            wrapMode: TextEdit.Wrap
            textFormat: TextEdit.PlainText
            text: page.text
            color: Theme.ink1
            selectionColor: Theme.selection
            selectedTextColor: Theme.ink1
            font: Theme.type.message
            Accessible.name: qsTr("Message text")
        }
    }
}
