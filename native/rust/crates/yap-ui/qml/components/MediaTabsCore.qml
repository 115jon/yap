import QtQuick
import Yap.Ui

// The open channel's Media, Links and Files (MemberList.tsx MediaTabContent,
// LinksTabContent, FilesTabContent), for the desktop's MediaTabs and the
// phone's MediaPage: the core's list (`Extras.media`, asked for afresh each
// time a tab shows), whether it is this channel's and tab's, its state
// (loading, failed, empty or rows), the empty line and glyph, a tile's
// animated test, the rows as the phone's viewer takes them, the jump and
// a link's opening. Each surface keeps its own grid, cards and states.
QtObject {
    id: core

    // "media", "links" or "files".
    property string tab: "media"
    // Asks again for the conversation opened under it (a surface that
    // stays up across a change; the desktop's details close instead).
    property bool follow: false

    readonly property var view: JSON.parse(Extras.media)
    readonly property bool current: view.tab === tab && view.channelId === Nav.activeChannelId
    readonly property var rows: current ? view.rows : []
    readonly property bool loading: !current || (view.loading && rows.length === 0)
    readonly property string error: current ? (view.error ?? "") : ""
    readonly property bool failed: !loading && rows.length === 0 && error.length > 0
    readonly property bool empty: !loading && rows.length === 0 && error.length === 0

    readonly property string emptyIcon: tab === "media" ? "image" : tab === "links" ? "link" : "fileText"
    readonly property string emptyText: tab === "media" ? qsTr("No media shared yet")
        : tab === "links" ? qsTr("No links shared yet") : qsTr("No files shared yet")

    // The jump was asked for (the surface closes itself).
    signal jumped(string messageId)

    function load() {
        Extras.loadMedia(core.tab)
    }
    onTabChanged: load()
    Component.onCompleted: load()
    property Connections following: Connections {
        target: Nav
        enabled: core.follow
        function onActiveChannelIdChanged() {
            if (Nav.activeChannelId.length > 0)
                core.load()
        }
    }

    // An animated WebP, by its link.
    function isWebp(url) {
        return /\.webp($|[?#])/i.test(url ?? "")
    }
    // A tile that plays in place: a GIF or an animated WebP, never a video.
    function isAnimated(row) {
        return !row.video && (row.gif === true || isWebp(row.url))
    }

    // The rows as the phone's viewer reads MessageMedia's images: a video
    // by its kind, its poster and who posted it kept.
    function viewerItems() {
        return core.rows.map(row => ({
            id: row.id,
            url: row.url,
            name: row.name,
            kind: row.video ? "video" : "image",
            gif: row.gif === true,
            poster: row.poster,
            messageId: row.messageId,
            author: row.author,
            avatar: row.avatar,
            createdMs: row.createdMs
        }))
    }

    // Shows the message in the chat.
    function jump(messageId) {
        if ((messageId ?? "").length === 0)
            return
        Chat.jumpToMessage(Nav.activeChannelId, messageId)
        jumped(messageId)
    }

    // A link from a message: one of ours (an invite, a channel here) opens
    // in the app and returns true; otherwise only a web link may leave it
    // (`web` says so), for the surface to open its own way.
    function openOwn(url) {
        return Invites.openLink(url)
    }
    function isWeb(url) {
        return /^https?:/i.test(url ?? "")
    }
}
