import QtQuick
import Yap.Ui

// The profile's relationship and its actions (ProfileCard.tsx
// ProfileActionRow's state), for the desktop's ProfileFriendActions and the
// phone's ProfilePage; each draws its own keys. The relationship comes
// from the core's `Friends.state` card:
//   friend    Message; Remove friend and Block
//   incoming  Accept, Message and Decline
//   outgoing  Message and Cancel request
//   none      Message and Add friend
//   blocked   Unblock and the blocked notice (no Message: the server
//             refuses a DM either way, #312)
//   self      nothing
// Block asks first (`confirmBlock`). While one action runs the others wait
// (`pending`); `act` is the person's `{busy, error, failed, done}`.
QtObject {
    id: core

    property string userId: ""
    property string username: ""
    property string name: ""

    readonly property var model: {
        try {
            return JSON.parse(Friends.state)
        } catch (e) {
            return {}
        }
    }
    readonly property string relation: model.card && model.card.userId === userId ? model.card.relation : "none"
    readonly property var act: model.acts?.[userId] ?? { busy: "", error: "", failed: "", done: 0 }
    readonly property bool pending: act.busy.length > 0
    readonly property bool blocked: relation === "blocked"
    // Block asked, not yet confirmed or backed out of.
    property bool confirmBlock: false

    readonly property string blockConsequence: qsTr("Block %1? They'll be removed from your friends and can't send you friend requests. You can unblock them later.").arg(name)
    readonly property string blockedNotice: qsTr("You blocked %1. They can't message you or send requests.").arg(name)

    // `add`, `accept`, `decline`, `cancel`, `remove`, `block` or `unblock`.
    function run(action) {
        Friends.act(action, userId, username, name)
    }

    // A fresh card starts clean (the web's hook state is per card).
    onUserIdChanged: {
        confirmBlock = false
        Friends.clear(userId)
    }
    onRelationChanged: if (relation === "blocked") confirmBlock = false
}
