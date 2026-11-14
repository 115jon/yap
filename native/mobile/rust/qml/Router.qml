pragma Singleton
import QtQuick

// The page router: any page or row opens a page by its file's name
// (`Router.open("SearchPage")` pushes SearchPage.qml over the pager), and
// Main.qml stacks it. A new page is a new file here and in build.rs, with
// no change to Main.qml: Main connects the signals every page may have by
// their names (closeRequested, profileRequested(userId),
// profileEditRequested, messageRequested(userId), serverOpened,
// conversationRequested(channelId); see Main.wire), and Android's back
// steps out of it like any other page.
//
// `options`, all optional:
// - `sheet`: rises from the bottom edge (a profile) rather than sliding in
//   from the right.
// - `key`: one page per name and key on top: opening the page on top
//   again (the same name and key) does nothing.
// - `whileBusy`: open even while the stack still animates (the default
//   drops a second tap that lands mid-transition).
// - `transition`: a StackView operation for the push.
QtObject {
    signal openRequested(string name, var props, var options)

    function open(name, props, options) {
        openRequested(name, props ?? {}, options ?? {})
    }
}
