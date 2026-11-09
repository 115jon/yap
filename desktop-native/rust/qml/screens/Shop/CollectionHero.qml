import QtQuick
import com.yap.native

// A collection's hero (ShopView.tsx CollectionHero): the hero art over the
// collection's colours, a scrim from the left so the white type reads,
// then the status chip and item count, the logo (or the name), the
// marketing line and, on Featured, "Browse collection".
Rectangle {
    id: root

    required property var hero

    signal browse(string id)

    implicitHeight: 300
    radius: Theme.radiusSheet * 2
    color: Theme.ply2
    border.width: 1
    border.color: Theme.line
    clip: true
    Accessible.role: Accessible.Grouping
    Accessible.name: hero.name

    CollectionGround {
        anchors.fill: parent
        anchors.margins: 1
        gradientInfo: root.hero.bg
    }
    Image {
        anchors.fill: parent
        anchors.margins: 1
        source: root.hero.art
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        sourceSize.width: 1600
    }
    Rectangle {
        anchors.fill: parent
        anchors.margins: 1
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: Qt.rgba(0, 0, 0, 0.8) }
            GradientStop { position: 0.55; color: Qt.rgba(0, 0, 0, 0.25) }
            GradientStop { position: 1; color: Qt.rgba(0, 0, 0, 0) }
        }
    }

    Column {
        x: 32
        anchors.verticalCenter: parent.verticalCenter
        width: Math.min(parent.width * 0.52, parent.width - 64)
        spacing: 12

        Row {
            spacing: 8
            StatusChip {
                visible: root.hero.status.length > 0
                label: root.hero.status
                tone: root.hero.tone
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: root.hero.count === 1 ? qsTr("1 item") : qsTr("%1 items").arg(root.hero.count)
                color: Qt.rgba(1, 1, 1, 0.8)
                font: Theme.type.captionStrong
            }
        }
        Image {
            id: logo
            visible: root.hero.logo.length > 0 && status !== Image.Error
            height: 64
            width: Math.min(parent.width * 0.7, implicitWidth > 0 ? implicitWidth * 64 / Math.max(1, implicitHeight) : parent.width * 0.7)
            source: root.hero.logo
            fillMode: Image.PreserveAspectFit
            horizontalAlignment: Image.AlignLeft
            asynchronous: true
            sourceSize.height: 136
        }
        Text {
            visible: !logo.visible
            width: parent.width
            textFormat: Text.PlainText
            text: root.hero.name
            wrapMode: Text.Wrap
            color: "#ffffff"
            font: Theme.type.sectionTitle
            Accessible.role: Accessible.Heading
        }
        Text {
            visible: text.length > 0
            width: parent.width
            textFormat: Text.PlainText
            text: root.hero.text
            wrapMode: Text.Wrap
            maximumLineCount: 3
            elide: Text.ElideRight
            color: Qt.rgba(1, 1, 1, 0.85)
            font: Theme.type.description
            lineHeight: 20
            lineHeightMode: Text.FixedHeight
        }
        KitButton {
            visible: root.hero.browse === true
            // ShopView.tsx: the default (md) primary with its cap and a
            // trailing arrow. The notch would show a ply over the art, so
            // the corner stays square here.
            variant: "primary"
            ground: "transparent"
            trailingIconName: "arrowRight"
            text: qsTr("Browse collection")
            onClicked: root.browse(root.hero.id)
        }
    }
}
