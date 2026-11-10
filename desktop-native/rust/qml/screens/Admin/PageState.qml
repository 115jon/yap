import QtQuick
import com.yap.native
import Yap.Ui

// A whole page's empty or failed state (server-settings/states.tsx
// PageState, LoadError; ui/empty-state.tsx): the EmptyState anatomy
// top-aligned 48 under the page header (pt-7 on the page's padding): the
// still `empty` DitherGlyph, the title, what it means at a 40ch measure,
// and Try again for a failure. `small` is the sm anatomy (a list inside a
// card: 15 px title, 12 gaps, py-6); `art` false drops the glyph.
Column {
    id: root

    property string title
    property string message
    property bool failed: false
    property bool retrying: false
    property bool small: false
    property bool art: true

    signal retry()

    width: parent ? parent.width : 0
    topPadding: small ? 24 : 28
    bottomPadding: small ? 24 : 40
    spacing: small ? 12 : 16
    Accessible.role: failed ? Accessible.AlertMessage : Accessible.StaticText
    Accessible.name: title
    Accessible.description: message

    Image {
        visible: root.art
        anchors.horizontalCenter: parent.horizontalCenter
        // DitherGlyph `empty`: 26 x 20 cells, 3 px (2 when small).
        width: 26 * (root.small ? 2 : 3)
        height: 20 * (root.small ? 2 : 3)
        smooth: false
        source: "image://dither/empty/" + Theme.hex(Theme.ink3) + "/" + Theme.hex(Theme.accentInk)
        Accessible.ignored: true
    }
    Column {
        // max-w-[40ch] of the 16 px body.
        width: Math.min(parent.width, 424)
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: 4
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            textFormat: Text.PlainText
            text: root.title
            wrapMode: Text.Wrap
            color: Theme.ink1
            font: root.small ? Theme.type.author : Theme.type.sectionTitle
            lineHeight: root.small ? 20 : 24
            lineHeightMode: Text.FixedHeight
        }
        Text {
            visible: root.message.length > 0
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            textFormat: Text.PlainText
            text: root.message
            wrapMode: Text.Wrap
            color: Theme.ink2
            font: Theme.type.description
            lineHeight: Theme.type.descriptionLine
            lineHeightMode: Text.FixedHeight
        }
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
