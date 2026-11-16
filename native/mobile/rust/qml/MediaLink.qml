pragma Singleton
import QtQuick

// The chat's player sheet (MediaSheet.qml, one in Main.qml): the seams
// (seams/InlineVideo.qml, seams/WebEmbedPlayer.qml) ask for it here, from
// inside a list row that may go while it plays.
//
// - playVideo({ source, startAt, loop, title, aspect, link }): a video
//   file (an attachment, a direct link, TikTok, Instagram or X media);
//   `link` is the page it is from, opened outside the app when the file
//   cannot be had.
// - playWeb({ url, link, title, aspect }): a web embed's player page (the
//   desktop WebPlayer's `url`); `link` opens outside the app where the
//   page cannot play here.
QtObject {
    signal videoRequested(var request)
    signal webRequested(var request)

    function playVideo(request) {
        videoRequested(request)
    }
    function playWeb(request) {
        webRequested(request)
    }
}
