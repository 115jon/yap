import QtQuick
import Yap.Ui

// Create a server (CreateServerModal.tsx) for both apps: the desktop's
// CreateServerDialog and the phone's CreateServerPage draw it their own
// way on this state. An optional icon (a picked file, checked first: the
// web's copy for a wrong type or one over 8 MB), then the name; the icon
// goes up first, then the server, which the core opens once created
// (`created` fires then). An empty name is caught inline (`nameError`),
// until the name is typed in again (`edited`).
QtObject {
    id: creator

    property string iconPath: ""
    property string iconError: ""
    // Typing clears the name error (SET_NAME), until the next create.
    property bool nameEdited: false
    // The core's finished count when this form started: a higher one is
    // its server, created.
    property int doneCount: 0
    // Only a started form hears its creates (a count from before `start`
    // is another form's).
    property bool started: false

    readonly property var model: JSON.parse(Invites.state)
    readonly property var act: model.create ?? { busy: false, error: "", done: 0 }
    readonly property bool busy: act.busy
    readonly property string nameError: nameEdited ? "" : model.createNameError ?? ""
    // The icon's problem first, else why the create failed.
    readonly property string error: iconError.length > 0 ? iconError : act.error
    readonly property string iconUrl: iconPath.length === 0 ? ""
        : iconPath.startsWith("/") ? "file://" + iconPath
        : "file:///" + iconPath.replace(/\\/g, "/")
    // The web's icon limit (CreateServerModal.tsx, 8 MB), for a picker
    // that holds a pick to it.
    readonly property real iconLimit: 8 * 1024 * 1024
    readonly property string iconHint: qsTr("PNG, JPG, GIF or WebP, up to 8 MB.")

    signal created()

    function start() {
        iconPath = ""
        iconError = ""
        nameEdited = false
        Invites.startCreate()
        doneCount = act.done
        started = true
    }
    function create(name) {
        if (busy)
            return
        iconError = ""
        nameEdited = false
        Invites.createServer(name, iconPath)
    }
    // A picked file: kept as the icon when it can be one, else its
    // problem shows. False when it can't.
    function takeIcon(path) {
        if (path.length === 0)
            return false
        const problem = Invites.iconProblem(path)
        iconError = problem
        if (problem.length === 0)
            iconPath = path
        return problem.length === 0
    }
    function removeIcon() {
        iconPath = ""
        iconError = ""
    }
    function edited() {
        nameEdited = true
    }

    onActChanged: {
        if (!started)
            return
        if (act.done < doneCount)
            doneCount = act.done
        else if (act.done > doneCount) {
            doneCount = act.done
            created()
        }
    }
}
