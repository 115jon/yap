pragma Singleton
import QtQuick

// The desktop's `Emoji` picker data. The message bar's quick reactions are
// `[{s: shortcode, u: picture}]` JSON from the core's emoji catalog; the
// bar is a hover affordance, so a phone shows none.
QtObject {
    readonly property string quick: "[]"
}
