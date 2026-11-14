pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// Links (widgets/Links.tsx): up to five labelled links (three on S), each
// a row with its host. They open in the browser only when clicked; the
// owner wrote them, not Yap.
WidgetPlate {
    id: root

    required property var widget
    required property var boardData
    required property var ctx

    readonly property var links: widget.cfg?.links ?? []
    readonly property var shown: links.slice(0, size === "S" ? 3 : 5)
    readonly property int more: links.length - shown.length
    readonly property bool slim: root.body.width < 200

    kind: widget.type
    size: widget.size
    draft: widget.draft ?? false

    WidgetEmpty {
        visible: root.shown.length === 0
        icon: "link"
        title: root.ctx?.isOwner ? qsTr("No links yet") : qsTr("No links")
    }

    Column {
        visible: root.shown.length > 0
        width: parent.width
        Accessible.role: Accessible.List
        Accessible.name: qsTr("Links")

        Repeater {
            model: root.shown
            delegate: T.AbstractButton {
                id: link
                required property var modelData
                readonly property string url: modelData.url ?? ""
                readonly property string label: modelData.label ?? ""
                readonly property string host: Board.linkHost(url)
                readonly property bool showHost: host.length > 0 && host !== label && !root.slim

                width: parent ? parent.width : 0
                height: 36
                leftPadding: 4
                rightPadding: 4
                focusPolicy: Qt.StrongFocus
                hoverEnabled: true
                Accessible.role: Accessible.Link
                Accessible.name: label
                Accessible.description: url
                // Only https, as the server stores them (cleanUrl): never
                // another scheme from a stale or hostile board.
                onClicked: if (/^https:\/\//i.test(url)) Qt.openUrlExternally(url)

                HoverHandler { cursorShape: Qt.PointingHandCursor }

                background: Rectangle {
                    radius: Theme.radius
                    color: link.hovered ? Theme.tint1 : "transparent"
                    border.width: link.visualFocus ? 2 : 0
                    border.color: Theme.focus
                }

                contentItem: Item {
                    Icon {
                        id: glyph
                        anchors.verticalCenter: parent.verticalCenter
                        name: "link"
                        size: Theme.iconSm
                        color: Theme.ink3
                    }
                    Text {
                        id: labelText
                        anchors.verticalCenter: parent.verticalCenter
                        x: glyph.width + 10
                        width: parent.width - x - 10 - (hostText.visible ? hostText.width + 10 : 0) - exit.width
                        textFormat: Text.PlainText
                        text: link.label
                        elide: Text.ElideRight
                        color: Theme.ink1
                        font: Theme.type.row
                    }
                    Text {
                        id: hostText
                        visible: link.showHost
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.right: exit.left
                        anchors.rightMargin: 10
                        width: Math.min(implicitWidth, parent.width * 0.45)
                        textFormat: Text.PlainText
                        text: link.host
                        elide: Text.ElideRight
                        color: Theme.ink3
                        font: Theme.type.numSm
                    }
                    Icon {
                        id: exit
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.right: parent.right
                        name: "externalLink"
                        size: Theme.iconXs
                        color: link.hovered ? Theme.ink1 : Theme.ink3
                    }
                }
            }
        }
    }

    Text {
        visible: root.more > 0 && root.shown.length > 0
        anchors.bottom: parent.bottom
        width: parent.width
        textFormat: Text.PlainText
        text: root.more === 1 ? qsTr("+1 more link") : qsTr("+%1 more links").arg(root.more)
        elide: Text.ElideRight
        color: Theme.ink3
        font: Theme.type.caption
    }
}
