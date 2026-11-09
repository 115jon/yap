pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// A too-big card (toobig-spec.md), inline where the file was picked, never
// a toast: Yappy's pose, the headline, the line, the size meter when there
// is room, and the actions. `info` is a card from `Limits` or a tray row
// (yap-core `limits.rs`: pose, headline, line, lineRich, hint, actions,
// size, limit, sizeText, limitText, alert). `compact` is the tray's tile:
// the small pose, no meter. Screen readers get it as an alert, the full
// sentence as its name; the keys keep the normal focus order.
Rectangle {
    id: root

    property var info: ({})
    property bool compact: false
    property bool meter: !compact
    property int cell: compact ? 2 : 3
    property font headlineFont: compact ? Theme.type.labelStrong : Theme.type.author
    property font lineFont: compact ? Theme.type.caption : Theme.type.description
    // Ground and edge: ply 2 on the dialogs, the warning edge in the tray.
    property color ground: Theme.ply2
    property color edge: Theme.line

    // An action key's id: `remove`, `choose`, `pick` or `removeBig`.
    signal action(string id)

    readonly property var actions: info.actions ?? []
    readonly property bool hasMeter: meter && (info.limit ?? 0) > 0 && (info.size ?? 0) > 0

    function label(id) {
        return id === "remove" ? qsTr("Remove") : id === "choose" ? qsTr("Choose another")
            : id === "pick" ? qsTr("Pick another") : id === "removeBig" ? qsTr("Remove the big ones")
            : id
    }

    // Callers set the width; the copy wraps to it.
    implicitWidth: compact ? 380 : cell >= 4 ? 560 : 440
    implicitHeight: row.implicitHeight + 2 * row.y
    radius: Theme.radius
    color: ground
    border.width: 1
    border.color: edge

    Accessible.role: Accessible.AlertMessage
    Accessible.name: info.alert ?? ""

    Row {
        id: row
        x: root.compact ? 10 : root.cell >= 4 ? 24 : 14
        y: x
        width: root.width - 2 * x
        spacing: root.compact ? 12 : root.cell >= 4 ? 22 : 16

        YappyPose {
            id: mascot
            anchors.verticalCenter: parent.verticalCenter
            pose: root.info.pose ?? "stuffed"
            cell: root.cell
        }
        Column {
            id: copy
            anchors.verticalCenter: parent.verticalCenter
            width: row.width - (mascot.visible ? mascot.width + row.spacing : 0)
            spacing: 4

            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: root.info.headline ?? ""
                elide: root.compact ? Text.ElideMiddle : Text.ElideNone
                wrapMode: root.compact ? Text.NoWrap : Text.Wrap
                color: Theme.ink1
                font: root.headlineFont
                Accessible.ignored: true
            }
            Text {
                width: parent.width
                visible: text.length > 0
                textFormat: Text.StyledText
                text: root.info.lineRich ?? ""
                wrapMode: Text.Wrap
                color: Theme.ink2
                font: root.lineFont
                Accessible.ignored: true
            }
            Item {
                visible: root.hasMeter
                width: Math.min(parent.width, 260)
                height: sizeMeter.implicitHeight + 10
                SizeMeter {
                    id: sizeMeter
                    y: 6
                    width: parent.width
                    size: root.info.size ?? 0
                    limit: root.info.limit ?? 1
                    sizeText: root.info.sizeText ?? ""
                    limitText: root.info.limitText ?? ""
                }
            }
            Text {
                width: parent.width
                visible: text.length > 0
                topPadding: 4
                textFormat: Text.PlainText
                text: root.info.hint ?? ""
                wrapMode: Text.Wrap
                color: Theme.ink3
                font: Theme.type.caption
                Accessible.ignored: true
            }
            Row {
                visible: root.actions.length > 0
                topPadding: root.compact ? 4 : 6
                spacing: 6
                Repeater {
                    model: root.actions
                    KitButton {
                        required property string modelData
                        size: "sm"
                        variant: "secondary"
                        text: root.label(modelData)
                        onClicked: root.action(modelData)
                    }
                }
            }
        }
    }
}
