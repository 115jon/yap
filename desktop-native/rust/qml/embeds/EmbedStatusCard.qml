import QtQuick
import com.yap.native
import Yap.Ui

// A post that cannot show (EmbedStatus.tsx): the header, then a 40 px
// tile with the state's glyph, its label as an LED label (unlit for gone,
// idle for restricted, muted for a failure), the sentence, the author when
// known, and the link as people read it. "Try again" only when another
// attempt can help. 400 wide.
Rectangle {
    id: root

    required property var embed
    property int maxWidth: 640

    signal openLink(string url)
    signal retry()

    readonly property string state_: embed.state ?? "gone"

    width: Math.min(maxWidth, 400)
    height: body.implicitHeight + 24
    radius: Theme.radius
    color: Theme.ply2
    border.width: 1
    border.color: Theme.line
    Accessible.role: root.state_ === "loading" ? Accessible.Indicator : Accessible.Grouping
    Accessible.name: embed.label + ". " + embed.text

    Column {
        id: body
        x: 12
        y: 12
        width: parent.width - 24
        spacing: 8
        EmbedHeader {
            width: parent.width
            provider: root.embed.provider
            colour: ""
            openTip: root.embed.open
            url: root.embed.url
            onOpenLink: url => root.openLink(url)
        }
        Row {
            spacing: 12
            width: parent.width
            Rectangle {
                width: 40
                height: 40
                radius: Theme.radius
                color: Theme.tint1
                Icon {
                    anchors.centerIn: parent
                    name: root.state_ === "restricted" ? "lock"
                        : root.state_ === "failed" ? "warning"
                        : root.state_ === "loading" ? "clock" : "linkBroken"
                    size: Theme.iconLg
                    color: Theme.ink3
                }
            }
            Column {
                width: parent.width - 52
                spacing: 2
                LedLabel {
                    text: root.embed.label
                    ledState: root.state_ === "restricted" ? "idle" : root.state_ === "failed" ? "live" : "off"
                }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: root.embed.text
                    wrapMode: Text.Wrap
                    color: Theme.ink2
                    font: Theme.type.description
                }
                Text {
                    width: parent.width
                    visible: text.length > 0
                    textFormat: Text.PlainText
                    text: root.embed.author ?? ""
                    elide: Text.ElideRight
                    color: Theme.ink1
                    font: Theme.type.captionStrong
                }
                Row {
                    spacing: 12
                    width: parent.width
                    KitButton {
                        visible: root.embed.retry === true
                        variant: "secondary"
                        size: "sm"
                        text: qsTr("Try again")
                        onClicked: root.retry()
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - (root.embed.retry === true ? 100 : 0)
                        textFormat: Text.PlainText
                        text: root.embed.short
                        elide: Text.ElideRight
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                }
            }
        }
    }
}
