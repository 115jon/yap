import QtQuick
import com.yap.native
import Yap.Ui

// An embed's header row (EmbedChrome.tsx EmbedHeader): a 6 px dot in the
// embed's colour, the site, "· date" when the post has one (EmbedDate:
// the time today, Yesterday, "Jan 26", or with the year), and the open key
// at the end.
Item {
    id: head

    property string provider
    property string colour
    property string openTip
    property string url
    // The post's ISO time, or empty.
    property string timestamp: ""

    // message-time.ts formatShortDate.
    readonly property string dateText: {
        if (timestamp.length === 0)
            return ""
        const d = new Date(timestamp)
        if (isNaN(d.getTime()))
            return ""
        const now = new Date()
        const dayOf = x => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime()
        const today = dayOf(now)
        const day = dayOf(d)
        if (day === today)
            return d.toLocaleTimeString(Qt.locale("en_US"), "h:mm AP")
        if (day === today - 86400000 || day === dayOf(new Date(today - 3600000)))
            return qsTr("Yesterday")
        const months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
        const md = months[d.getMonth()] + " " + d.getDate()
        return d.getFullYear() === now.getFullYear() ? md : md + ", " + d.getFullYear()
    }

    signal openLink(string url)

    implicitHeight: 20
    visible: provider.length > 0 || url.length > 0

    Rectangle {
        id: dot
        anchors.verticalCenter: parent.verticalCenter
        width: 6
        height: 6
        radius: 3
        color: head.colour.length > 0 ? head.colour : Theme.lineStrong
    }
    Row {
        anchors.left: dot.right
        anchors.leftMargin: 8
        anchors.right: openKey.left
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        spacing: 8
        Text {
            id: site
            width: Math.min(implicitWidth, parent.width - (stamp.visible ? stamp.implicitWidth + sep.implicitWidth + 16 : 0))
            textFormat: Text.PlainText
            text: head.provider
            elide: Text.ElideRight
            color: Theme.ink2
            font: Theme.type.captionStrong
        }
        Text {
            id: sep
            visible: stamp.visible
            text: "·"
            color: Theme.ink3
            font: Theme.type.caption
            Accessible.ignored: true
        }
        Text {
            id: stamp
            visible: head.dateText.length > 0
            textFormat: Text.PlainText
            text: head.dateText
            color: Theme.ink3
            font: Theme.type.caption
        }
    }
    IconKey {
        id: openKey
        anchors.right: parent.right
        anchors.rightMargin: -4
        anchors.verticalCenter: parent.verticalCenter
        visible: head.url.length > 0
        size: 28
        tone: "muted"
        iconName: "externalLink"
        tip: head.openTip
        onClicked: head.openLink(head.url)
    }
}
