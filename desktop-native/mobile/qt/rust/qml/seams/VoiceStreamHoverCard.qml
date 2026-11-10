import QtQuick

// The desktop's stream preview on hover: there is no hover on a phone, and
// no voice in the prototype yet.
Item {
    property Item anchorItem: null
    property bool me: false
    property string name: ""
    property string preview: ""
    property bool held: false

    signal watchRequested()

    visible: false
}
