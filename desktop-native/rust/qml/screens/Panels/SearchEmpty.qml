import QtQuick
import com.yap.native

// The panel's failure and empty states (ui/empty-state.tsx, size sm,
// py-12): the title, what it means, and one secondary action.
Column {
    id: root

    property string title
    property string message
    property string actionText
    property string actionIcon

    signal action()

    topPadding: 48
    bottomPadding: 48
    spacing: 4
    Accessible.role: Accessible.StaticText
    Accessible.name: title
    Accessible.description: message

    Text {
        width: parent.width
        horizontalAlignment: Text.AlignHCenter
        textFormat: Text.PlainText
        text: root.title
        wrapMode: Text.Wrap
        color: Theme.ink1
        font: Theme.type.panelTitle
    }
    Text {
        x: 24
        width: parent.width - 48
        horizontalAlignment: Text.AlignHCenter
        textFormat: Text.PlainText
        text: root.message
        wrapMode: Text.Wrap
        color: Theme.ink3
        font: Theme.type.description
        lineHeight: Theme.type.descriptionLine
        lineHeightMode: Text.FixedHeight
    }
    Item {
        visible: root.actionText.length > 0
        width: 1
        height: 12
    }
    KitButton {
        visible: root.actionText.length > 0
        anchors.horizontalCenter: parent.horizontalCenter
        variant: "secondary"
        size: "sm"
        iconName: root.actionIcon
        text: root.actionText
        onClicked: root.action()
    }
}
