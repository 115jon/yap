import QtQuick
import com.yap.native

// A whole page's empty or failed state (server-settings/states.tsx
// PageState, LoadError): the EmptyState anatomy top-aligned 48 under the
// page header, the title, what it means, and Try again for a failure.
Column {
    id: root

    property string title
    property string message
    property bool failed: false
    property bool retrying: false

    signal retry()

    width: parent ? parent.width : 0
    topPadding: 16
    spacing: 8
    Accessible.role: failed ? Accessible.AlertMessage : Accessible.StaticText
    Accessible.name: title
    Accessible.description: message

    Text {
        width: parent.width
        horizontalAlignment: Text.AlignHCenter
        textFormat: Text.PlainText
        text: root.title
        wrapMode: Text.Wrap
        color: Theme.ink1
        font: Theme.type.sectionTitle
    }
    Text {
        width: Math.min(parent.width, 440)
        anchors.horizontalCenter: parent.horizontalCenter
        horizontalAlignment: Text.AlignHCenter
        textFormat: Text.PlainText
        text: root.message
        wrapMode: Text.Wrap
        color: Theme.ink2
        font: Theme.type.description
        lineHeight: Theme.type.descriptionLine
        lineHeightMode: Text.FixedHeight
    }
    Item {
        visible: root.failed
        width: 1
        height: 8
    }
    KitButton {
        visible: root.failed
        anchors.horizontalCenter: parent.horizontalCenter
        variant: "secondary"
        text: qsTr("Try again")
        loading: root.retrying
        onClicked: root.retry()
    }
}
