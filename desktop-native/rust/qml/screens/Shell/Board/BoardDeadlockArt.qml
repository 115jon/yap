import QtQuick
import com.yap.native

// One mirrored Deadlock image (Deadlock.tsx HeroIcon / RankBadge / item):
// `cover` crops a hero face or an item onto ply-4; otherwise a rank badge
// fits its box with no ground. A missing or broken image shows ply-4 and,
// for a hero, its initial. The caller sizes it.
Item {
    id: art

    property string source: ""
    property bool cover: true
    property string initial: ""
    // The accessible name; empty leaves the image out of the tree.
    property string alt: ""
    readonly property bool failed: source.length === 0 || img.status === Image.Error

    Accessible.role: Accessible.Graphic
    Accessible.name: alt
    Accessible.ignored: alt.length === 0

    Rectangle {
        anchors.fill: parent
        visible: art.cover || art.failed
        radius: Theme.radius
        color: Theme.ply4
    }
    Image {
        id: img
        anchors.fill: parent
        visible: !art.failed
        source: art.source
        fillMode: art.cover ? Image.PreserveAspectCrop : Image.PreserveAspectFit
        sourceSize.width: Math.max(16, Math.ceil(art.width * 2))
        asynchronous: true
        cache: true
        smooth: true
        mipmap: true
    }
    Text {
        anchors.centerIn: parent
        visible: art.failed && art.initial.length > 0
        textFormat: Text.PlainText
        text: art.initial.charAt(0)
        color: Theme.ink3
        font: Qt.font({ family: "Yap Bricolage Head", pixelSize: art.height >= 40 ? 18 : 11, weight: Font.Bold })
    }
}
