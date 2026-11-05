import QtQuick
import com.yap.native

// A confirm that runs its request from the confirm key and stays open with
// the reason when it fails (ui/confirm-dialog.tsx): delete channel, delete
// category, kick, and ban with its optional reason (BanMemberDialog.tsx).
// The confirm is the solid danger fill when it can't be undone.
PlateDialog {
    id: dialog

    // The action key in Admin's acts; `run(reason)` sends the request.
    property string actKey
    property string confirmLabel
    property bool irreversible: true
    property bool askReason: false
    property var run: null
    property int doneCount: 0

    readonly property var model: JSON.parse(Admin.state)
    readonly property var act: model.acts[actKey] ?? { busy: false, error: "", done: 0 }

    function ask(key, heading, text, label, sure, withReason, request) {
        actKey = key
        title = heading
        description = text
        confirmLabel = label
        irreversible = sure
        askReason = withReason
        run = request
        reasonField.text = ""
        Admin.clear(key)
        doneCount = act.done
        open()
    }

    busy: act.busy
    onOpened: (askReason ? reasonField : confirmKey).forceActiveFocus()
    onActChanged: {
        if (act.done < doneCount)
            doneCount = act.done
        else if (opened && act.done > doneCount) {
            doneCount = act.done
            close()
        }
    }

    FieldPlate {
        visible: dialog.askReason
        width: parent.width
        label: qsTr("Reason (optional)")
        ground: Theme.ply3
        KitField {
            id: reasonField
            maximumLength: 512
            Accessible.name: qsTr("Reason (optional)")
            Keys.onReturnPressed: confirmKey.clicked()
        }
    }
    RowError {
        above: 0
        text: dialog.act.error
    }

    footer: [
        KitButton {
            variant: "ghost"
            text: qsTr("Cancel")
            enabled: !dialog.act.busy
            onClicked: dialog.close()
        },
        KitButton {
            id: confirmKey
            variant: dialog.irreversible ? "danger" : "destructive"
            text: dialog.confirmLabel
            loading: dialog.act.busy
            onClicked: if (!dialog.act.busy && dialog.run) dialog.run(reasonField.text)
        }
    ]
}
