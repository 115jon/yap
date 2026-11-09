import QtQuick
import com.yap.native

// Remove friend, Ignore request or Cancel request, confirmed first
// (FriendsView.tsx `removeCopy` and its ResponsiveDialog): Keep, and the
// danger key, which shows the spinner while the request runs. It closes
// once the request succeeds; on failure it stays open, as the web's does
// (the web shows no reason there).
PlateDialog {
    id: dialog

    property string userId
    property string name
    property string kind
    // The person's act from Friends (`{busy, error, failed, done}`).
    property var act: ({ busy: "", done: 0 })
    property int doneCount: 0

    readonly property bool running: act.busy.length > 0

    function ask(person) {
        userId = person.userId
        name = person.name
        kind = person.kind
        doneCount = act.done
        open()
    }

    width: Math.min(384, parent.width - 32)
    title: kind === "friend" ? qsTr("Remove friend")
        : kind === "incoming" ? qsTr("Ignore request") : qsTr("Cancel request")
    description: kind === "friend" ? qsTr("%1 will be removed from your friends. You can send a new request later.").arg(name)
        : kind === "incoming" ? qsTr("Ignore the friend request from %1?").arg(name)
        : qsTr("Cancel your friend request to %1?").arg(name)
    busy: running
    onOpened: keep.forceActiveFocus()
    onActChanged: {
        if (act.done < doneCount)
            doneCount = act.done
        else if (opened && act.done > doneCount) {
            doneCount = act.done
            close()
        }
    }

    footer: [
        KitButton {
            id: keep
            variant: "secondary"
            text: qsTr("Keep")
            enabled: !dialog.running
            onClicked: dialog.close()
        },
        KitButton {
            variant: "danger"
            text: dialog.kind === "friend" ? qsTr("Remove")
                : dialog.kind === "incoming" ? qsTr("Ignore") : qsTr("Cancel request")
            loading: dialog.running
            onClicked: {
                if (dialog.running)
                    return
                const action = dialog.kind === "friend" ? "remove" : dialog.kind === "incoming" ? "decline" : "cancel"
                Friends.act(action, dialog.userId, "", dialog.name)
            }
        }
    ]
}
