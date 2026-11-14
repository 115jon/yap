import QtQuick
import Yap.Ui

// Create channel (CreateChannelModal.tsx) for both apps: the desktop's
// CreateChannelDialog and the phone's CreateChannelPage draw it their own
// way on this state. Text or voice, a name (a text channel's turns
// lowercase with dashes as it is typed: `clean`), in the category it was
// started in, or none. The open server's; an empty name is caught inline
// (`nameError`); a new text channel opens once created (`created` fires).
QtObject {
    id: creator

    property string categoryId: ""
    property bool voice: false
    // The core's finished count when this form started: a higher one is
    // its channel, created.
    property int doneCount: 0

    readonly property var model: JSON.parse(Admin.state)
    readonly property var act: model.acts["create-channel"] ?? { busy: false, error: "", done: 0 }
    readonly property bool busy: act.busy
    readonly property string error: act.error
    readonly property string nameError: model.acts["create-channel.name"]?.error ?? ""
    readonly property string categoryName: (model.categories ?? []).find(category => category.id === categoryId)?.name ?? ""
    readonly property string placeLine: categoryName.length > 0 ? qsTr("In %1").arg(categoryName) : qsTr("Not in a category")
    readonly property string placeholder: voice ? qsTr("General Voice") : qsTr("new-channel")
    readonly property string textHint: qsTr("Send messages, images, GIFs, and more.")
    readonly property string voiceHint: qsTr("Hang out with voice, video, and screen share.")

    signal created()

    function start(category) {
        categoryId = category ?? ""
        voice = false
        Admin.clear("create-channel")
        Admin.clear("create-channel.name")
        doneCount = act.done
    }
    function create(name) {
        if (!busy)
            Admin.createChannel(name, voice, categoryId)
    }
    // The name as the type allows it while typing.
    function clean(name) {
        return Admin.sanitizeChannelName(name, voice, false)
    }
    // Typing clears the name error.
    function edited() {
        Admin.clear("create-channel.name")
    }

    onActChanged: {
        if (act.done < doneCount)
            doneCount = act.done
        else if (act.done > doneCount) {
            doneCount = act.done
            created()
        }
    }
}
