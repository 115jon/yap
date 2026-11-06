import QtQuick
import QtQuick.Templates as T
import com.yap.native

// A device select (SettingsSelect on the FieldPlate recipe): ply-2, a 1 px
// line-field edge (ink-2 on hover, ink-1 plus the ring on focus), 40 high
// (48 `large`), the chosen name and a caret. It opens the list as radio
// rows under it; the first entry is the system default, named after it
// with a "Default" hint (`firstIsDefault: false` for a plain list).
T.AbstractButton {
    id: control

    property list<string> ids
    property list<string> names
    property string current: ""
    // The default's name when the system did not give one.
    property string fallback: ""
    property bool firstIsDefault: true
    // The 48 px legend-plate control of a settings field row.
    property bool large: false
    readonly property int currentIndex: Math.max(0, ids.indexOf(current))

    signal picked(string id)

    function nameAt(index) {
        const name = names[index] ?? ""
        return control.firstIsDefault && index === 0 && name.length === 0 ? fallback : name
    }

    implicitWidth: 360
    implicitHeight: large ? Theme.controlLg : Theme.control
    leftPadding: 16
    rightPadding: 40
    focusPolicy: Qt.StrongFocus
    Accessible.role: Accessible.ComboBox
    Accessible.name: text
    Accessible.description: nameAt(currentIndex)

    onClicked: list.visible ? list.close() : list.popup(control, 0, control.height + 4)

    contentItem: Text {
        textFormat: Text.PlainText
        text: control.nameAt(control.currentIndex)
        elide: Text.ElideRight
        verticalAlignment: Text.AlignVCenter
        color: Theme.ink1
        font: Theme.type.row
    }

    indicator: Icon {
        x: control.width - width - 16
        y: (control.height - height) / 2
        name: "chevronDown"
        size: Theme.iconSm
        color: Theme.ink2
    }

    background: Rectangle {
        radius: Theme.radius
        color: Theme.ply2
        border.width: control.visualFocus ? 2 : 1
        border.color: control.visualFocus ? Theme.ink1 : control.hovered || list.visible ? Theme.ink2 : Theme.lineField
    }

    MenuPanel {
        id: list
        implicitWidth: control.width
        Instantiator {
            model: control.ids
            delegate: MenuChoice {
                required property int index
                required property string modelData
                text: control.nameAt(index)
                hint: control.firstIsDefault && index === 0 ? qsTr("Default") : ""
                selected: index === control.currentIndex
                onTriggered: control.picked(modelData)
            }
            onObjectAdded: (index, object) => list.insertItem(index, object)
            onObjectRemoved: (index, object) => list.removeItem(object)
        }
    }

    HoverHandler { cursorShape: Qt.PointingHandCursor }
}
