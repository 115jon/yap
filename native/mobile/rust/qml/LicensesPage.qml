pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import Yap.Ui

// Settings > About > Open source licences: the third-party notices and
// licence texts the APK carries (assets/licenses, Native.licenseFiles),
// one row per file, the notices first; a row opens its text on the same
// page. A text is laid out in runs of paragraphs, made as they scroll in:
// the Rust crates' file alone is over a megabyte. Back, and the predictive
// back gesture, go from a text to the list and from the list to Settings.
FocusScope {
    id: page

    property real back: 0
    property real shown: 0
    // The open file, or "" for the list.
    property string file: ""
    property var files: []
    property var chunks: []

    signal closed()

    property bool up: false
    onVisibleChanged: if (!visible && up) {
        // Hidden from above while open: gone at once, Back given back.
        fade.stop()
        Native.popBackLayer(page)
        up = false
        shown = 0
        back = 0
        closed()
    }

    function open() {
        files = Native.licenseFiles()
        showFile("")
        up = true
        forceActiveFocus()
        Native.pushBackLayer(page)
        fade.to = 1
        fade.restart()
    }
    function close() {
        if (!up || fade.to === 0)
            return
        Native.popBackLayer(page)
        fade.to = 0
        fade.restart()
    }
    function showFile(name) {
        file = name
        chunks = name.length > 0 ? split(Native.readLicense(name)) : []
        textList.positionViewAtBeginning()
    }
    // Runs of whole paragraphs (blank-line breaks), about 3000 characters
    // each, so one delegate never lays out a whole licence file.
    function split(text) {
        const paragraphs = text.replace(/\r\n/g, "\n").split(/\n{2,}/)
        const out = []
        let run = ""
        for (const p of paragraphs) {
            run = run.length > 0 ? run + "\n\n" + p : p
            if (run.length >= 3000) {
                out.push(run)
                run = ""
            }
        }
        if (run.length > 0)
            out.push(run)
        return out
    }
    function title(name) {
        return name === "THIRD_PARTY_NOTICES.md" ? qsTr("Third-party notices") : name
    }
    function backProgressed(progress) { back = progress }
    function backCancelled() { back = 0 }
    function backInvoked() {
        back = 0
        if (file.length > 0)
            showFile("")
        else
            close()
    }

    visible: up
    z: 110
    opacity: shown * (1 - back * 0.2)
    scale: 1 - back * 0.08
    transform: Translate { y: (1 - page.shown) * Theme.panelShift; x: page.back * page.width * 0.1 }
    Component.onDestruction: Native.popBackLayer(page)

    NumberAnimation {
        id: fade
        target: page
        property: "shown"
        duration: to === 1 ? Theme.dur4 : Theme.dur4Exit
        easing.type: Easing.BezierSpline
        easing.bezierCurve: Theme.easeOut
        onFinished: if (to === 0) {
            page.up = false
            page.back = 0
            page.file = ""
            page.chunks = []
            page.closed()
        }
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.ply0
    }
    // Nothing under the page takes a touch.
    MouseArea { anchors.fill: parent }

    PageHeader {
        id: header
        z: 1
        title: page.file.length > 0 ? page.title(page.file) : qsTr("Open source licences")
        topInset: page.SafeArea.margins.top
        leftInset: page.SafeArea.margins.left
        rightInset: page.SafeArea.margins.right
        onBackRequested: page.backInvoked()
    }

    // The files.
    ListView {
        id: fileList
        visible: page.file.length === 0
        anchors.top: header.bottom
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: 16 + page.SafeArea.margins.left
        anchors.rightMargin: 16 + page.SafeArea.margins.right
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickDeceleration: Native.flingDeceleration
        maximumFlickVelocity: Native.maxFlingVelocity
        model: page.files
        header: Text {
            width: fileList.width
            topPadding: 16
            bottomPadding: 12
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            text: page.files.length > 0
                ? qsTr("Yap is built with these open source libraries, fonts and icons. Their notices and licences are below.")
                : qsTr("This build carries no licence texts.")
            color: Theme.ink2
            font: Theme.type.description
        }
        footer: Item { width: 1; height: 16 + page.SafeArea.margins.bottom }
        delegate: Item {
            id: fileRow
            required property string modelData
            width: fileList.width
            height: 48
            Accessible.role: Accessible.Button
            Accessible.name: page.title(modelData)
            Accessible.onPressAction: page.showFile(modelData)
            Rectangle {
                anchors.fill: parent
                anchors.leftMargin: -8
                anchors.rightMargin: -8
                radius: Theme.radius
                color: rowTap.pressed ? Theme.tint2 : "transparent"
                Behavior on color { ColorAnimation { duration: Theme.dur1 } }
            }
            Text {
                anchors.left: parent.left
                anchors.right: chevron.left
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                elide: Text.ElideRight
                text: page.title(fileRow.modelData)
                color: Theme.ink1
                font: Theme.type.setting
            }
            Icon {
                id: chevron
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                name: "chevronRight"
                size: Theme.iconMd
                color: Theme.ink3
            }
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 1
                color: Theme.line
            }
            TapHandler {
                id: rowTap
                onTapped: {
                    Native.haptic(Native.Tick)
                    page.showFile(fileRow.modelData)
                }
            }
        }
    }

    // One file's text.
    ListView {
        id: textList
        visible: page.file.length > 0
        anchors.top: header.bottom
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: 16 + page.SafeArea.margins.left
        anchors.rightMargin: 16 + page.SafeArea.margins.right
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickDeceleration: Native.flingDeceleration
        maximumFlickVelocity: Native.maxFlingVelocity
        model: page.chunks
        header: Item { width: 1; height: 16 }
        footer: Item { width: 1; height: 16 + page.SafeArea.margins.bottom }
        delegate: Text {
            required property string modelData
            width: textList.width
            bottomPadding: 12
            wrapMode: Text.Wrap
            // The notices are Markdown; the licence texts plain, in mono so
            // their own layout (lists, tables of names) holds.
            textFormat: page.file.endsWith(".md") ? Text.MarkdownText : Text.PlainText
            text: modelData
            color: Theme.ink1
            font: page.file.endsWith(".md") ? Theme.type.message : Theme.type.code
            linkColor: Theme.accentInk
            onLinkActivated: link => {
                if (/^https:\/\//i.test(link))
                    Qt.openUrlExternally(link)
            }
        }
    }
}
