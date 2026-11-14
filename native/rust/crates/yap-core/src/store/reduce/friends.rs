//! Friends: the list (chat-actions.ts `loadRelationships`, at bootstrap and
//! after a fresh gateway session), FriendsView.tsx's add form and row
//! actions, the profile card's actions (ProfileCard.tsx
//! `useProfileActions`), and `RELATIONSHIP_ADD` / `RELATIONSHIP_REMOVE`
//! (chat-gateway.ts, chat-reducer.ts). Every successful action loads the
//! list again, as the web does; nothing is optimistic.

use yap_protocol::payload::UserProfileUpdate;
use yap_protocol::rest::Relationship;

use super::Effects;
use crate::api::ApiError;
use crate::settings::describe_api_error;
use crate::store::friends::{
    add_friend_error, friends_view, AddStatus, FriendAction, FriendsCommand, FriendsContext,
    FriendsOp, FriendsRequest, FriendsResponse,
};
use crate::store::state::State;
use crate::store::view::ViewUpdate;
use crate::store::{ApiRequest, ApiResponse, RequestId};

fn friends_response(result: Result<ApiResponse, ApiError>) -> Result<FriendsResponse, ApiError> {
    match result? {
        ApiResponse::Friends(response) => Ok(response),
        ApiResponse::Done => Ok(FriendsResponse::Done),
        _ => Err(ApiError::Decode("unexpected response".into())),
    }
}

impl State {
    /// Relationships as the store has them.
    pub fn friends(&self) -> &crate::store::FriendsState {
        &self.friends
    }

    fn friends_http(
        &mut self,
        fx: &mut Effects,
        request: FriendsRequest,
        op: FriendsOp,
    ) -> Option<RequestId> {
        let id = self.http(fx, ApiRequest::Friends(request))?;
        self.pending.friends.insert(id, op);
        Some(id)
    }

    /// Asks for the list; `shown` puts up the view's loading rows.
    pub(super) fn load_friends(&mut self, fx: &mut Effects, shown: bool) {
        if self.signed_in().is_none() {
            return;
        }
        if let Some(id) = self.friends_http(fx, FriendsRequest::List, FriendsOp::List) {
            self.friends.list = Some(id);
            self.friends.loading |= shown;
        }
    }

    pub(super) fn on_friends(&mut self, fx: &mut Effects, command: FriendsCommand) {
        match command {
            FriendsCommand::Load => self.load_friends(fx, true),
            FriendsCommand::Refresh => self.load_friends(fx, false),
            FriendsCommand::SendRequest(username) => self.send_friend_request(fx, &username),
            FriendsCommand::ClearAdd => {
                if self.friends.add.kind == "error" {
                    self.friends.add.kind = "";
                    self.friends.add.message.clear();
                }
            }
            FriendsCommand::Act {
                action,
                user_id,
                username,
                name,
            } => self.friend_act(fx, action, user_id, &username, name),
            FriendsCommand::Clear(user_id) => {
                let idle = self
                    .friends
                    .acts
                    .get(&user_id)
                    .is_some_and(|act| act.busy.is_empty());
                if idle {
                    self.friends.acts.remove(&user_id);
                }
            }
        }
    }

    /// FriendsView.tsx `handleAddFriend`: an empty field says so, else the
    /// trimmed username goes out. The web does not lock the form; the
    /// newest request's answer is the one shown.
    fn send_friend_request(&mut self, fx: &mut Effects, username: &str) {
        let username = username.trim();
        if username.is_empty() {
            self.friends.add.kind = "error";
            self.friends.add.message = "Enter a username.".into();
            return;
        }
        let request = FriendsRequest::Send {
            username: username.to_owned(),
        };
        self.friends.add_request = self.friends_http(fx, request, FriendsOp::Add);
    }

    /// One action on one person, one at a time (the card's `busy`).
    fn friend_act(
        &mut self,
        fx: &mut Effects,
        action: FriendAction,
        user_id: String,
        username: &str,
        name: String,
    ) {
        if user_id.is_empty() || self.me().is_some_and(|me| me.id == user_id) {
            return;
        }
        if self
            .friends
            .acts
            .get(&user_id)
            .is_some_and(|act| !act.busy.is_empty())
        {
            return;
        }
        let request = match action {
            FriendAction::Add => FriendsRequest::Send {
                username: username.trim().to_owned(),
            },
            FriendAction::Accept => FriendsRequest::Accept {
                user_id: user_id.clone(),
            },
            FriendAction::Block => FriendsRequest::Block {
                user_id: user_id.clone(),
            },
            FriendAction::Decline
            | FriendAction::Cancel
            | FriendAction::Remove
            | FriendAction::Unblock => FriendsRequest::Remove {
                user_id: user_id.clone(),
            },
        };
        let op = FriendsOp::Act {
            user_id: user_id.clone(),
            action,
            name,
        };
        if self.friends_http(fx, request, op).is_some() {
            let act = self.friends.acts.entry(user_id).or_default();
            act.busy = action.as_str();
            act.error.clear();
            act.failed = "";
        }
    }

    /// Answers for friends requests; `false` if `id` is not one.
    pub(super) fn route_friends_http(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        result: &mut Option<Result<ApiResponse, ApiError>>,
    ) -> bool {
        let Some(op) = self.pending.friends.remove(&id) else {
            return false;
        };
        let Some(result) = result.take() else {
            return true;
        };
        if let Err(error) = &result {
            if error.is_unauthorized() {
                self.fail(fx, error);
                return true;
            }
        }
        let result = friends_response(result);
        match op {
            FriendsOp::List => {
                if self.friends.list != Some(id) {
                    return true;
                }
                self.friends.list = None;
                self.friends.loading = false;
                // A failed load keeps the list (chat-actions.ts ignores it).
                if let Ok(FriendsResponse::List(list)) = result {
                    for rel in &list {
                        self.names
                            .entry(rel.user.id.clone())
                            .or_insert_with(|| rel.user.label().to_owned());
                    }
                    self.friends.relationships = list;
                }
            }
            FriendsOp::Add => {
                if self.friends.add_request != Some(id) {
                    return true;
                }
                self.friends.add_request = None;
                match result {
                    Ok(response) => {
                        let accepted = matches!(response, FriendsResponse::Sent(Some(0)));
                        self.friends.add = AddStatus {
                            kind: "success",
                            message: if accepted {
                                "Friend added.".into()
                            } else {
                                "Friend request sent.".into()
                            },
                            sent: self.friends.add.sent + 1,
                        };
                        self.load_friends(fx, true);
                    }
                    Err(error) => {
                        self.friends.add.kind = "error";
                        self.friends.add.message = add_friend_error(&error);
                    }
                }
            }
            FriendsOp::Act {
                user_id,
                action,
                name,
            } => {
                let ok = result.is_ok();
                let act = self.friends.acts.entry(user_id).or_default();
                act.busy = "";
                match result {
                    Ok(_) => act.done += 1,
                    Err(error) => {
                        act.error = describe_api_error(&error, &action.copy(&name));
                        act.failed = action.as_str();
                    }
                }
                if ok {
                    self.load_friends(fx, false);
                }
            }
        }
        true
    }

    /// `RELATIONSHIP_ADD`: replaces theirs, else goes on top.
    pub(super) fn on_relationship_add(&mut self, relationship: Relationship) {
        let list = &mut self.friends.relationships;
        list.retain(|r| r.user.id != relationship.user.id);
        self.names
            .entry(relationship.user.id.clone())
            .or_insert_with(|| relationship.user.label().to_owned());
        list.insert(0, relationship);
    }

    /// `RELATIONSHIP_REMOVE`.
    pub(super) fn on_relationship_remove(&mut self, user_id: &str) {
        self.friends.relationships.retain(|r| r.user.id != user_id);
    }

    /// A profile changed: their relationship row follows (chat-reducer.ts
    /// `UPDATE_USER_PROFILE` step 5).
    pub(super) fn on_friend_profile(&mut self, update: &UserProfileUpdate) {
        for rel in self
            .friends
            .relationships
            .iter_mut()
            .filter(|r| r.user.id == update.user_id)
        {
            update.apply(&mut rel.user);
        }
    }

    /// Sign-out: nothing of one account shows for the next.
    pub(super) fn clear_friends(&mut self) {
        self.friends = Default::default();
        self.pending.friends.clear();
    }

    /// Sends the friends view when it changed.
    pub(super) fn sync_friends(&mut self, fx: &mut Effects) {
        let view = friends_view(
            &self.friends,
            &FriendsContext {
                presence: &self.presence,
                api_base: &self.api_base,
                me_id: self.me().map_or("", |me| me.id.as_str()),
                card_user: self.profile_user(),
                dm_unread: self.view.dm_unread,
            },
        );
        if view != self.view.friends {
            self.view.friends = view.clone();
            fx.view(ViewUpdate::Friends(Box::new(view)));
        }
    }
}
