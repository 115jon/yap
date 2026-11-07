import QtQuick
import QtQuick.Templates as T
import com.yap.native

// The star over a GIF or clip in a message (GifFavoriteButton.tsx): a 32 px
// chip of black at 60 % over the media, the star filled once saved. A
// press saves or removes it at once (Chat.toggleGifFavorite); the list
// loads the first time a star shows. `favorite` is the GifPickerItem the
// core built for the media (its provider and id decide the state).
T.Button {
    id: control

    property var favorite: null

    readonly property string key: control.favorite ? control.favorite.provider + ":" + control.favorite.id : ""
    readonly property var saved: JSON.parse(Chat.gifFavorites.length > 0 ? Chat.gifFavorites : "[]")
    readonly property bool isSaved: control.key.length > 0 && control.saved.indexOf(control.key) >= 0
    readonly property bool isClip: control.favorite !== null
        && (control.favorite.mediaType === "clips" || control.favorite.send.contentType === "video/mp4")
    readonly property string label: control.isSaved
        ? (control.isClip ? qsTr("Remove clip from favorites") : qsTr("Remove GIF from favorites"))
        : (control.isClip ? qsTr("Add clip to favorites") : qsTr("Add GIF to favorites"))

    implicitWidth: 32
    implicitHeight: 32
    padding: 0
    scale: down ? Theme.scalePressIcon : 1
    Behavior on scale { NumberAnimation { duration: Theme.dur1; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }

    Component.onCompleted: Chat.loadGifFavorites()
    onClicked: if (control.favorite) Chat.toggleGifFavorite(JSON.stringify(control.favorite))

    Accessible.role: Accessible.Button
    Accessible.name: control.label
    Accessible.checkable: true
    Accessible.checked: control.isSaved

    Tooltip {
        visible: control.hovered
        text: control.label
        side: "right"
    }

    background: Rectangle {
        radius: Theme.radius
        color: Qt.rgba(0, 0, 0, control.hovered ? 0.72 : 0.6)
        Rectangle {
            anchors.fill: parent
            anchors.margins: -2
            visible: control.visualFocus
            color: "transparent"
            radius: Theme.radius + 2
            border.width: 2
            border.color: Theme.focus
        }
    }

    // Over media the chip is dark, where the accent ink can fall under 3:1:
    // the fill carries the state, in white.
    contentItem: Item {
        Icon {
            anchors.centerIn: parent
            name: "star"
            variant: control.isSaved ? "active" : "idle"
            size: Theme.iconMd
            color: "#ffffff"
        }
    }

    HoverHandler { cursorShape: Qt.PointingHandCursor }
}
