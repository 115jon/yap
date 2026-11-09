//! Game activity ("Playing X"): what others are playing, from presence
//! (chat-reducer.ts `gameActivityByUserId`), and our own, reported with
//! op 43 (chat-gateway.ts `setGameActivity`).
//!
//! Ours is sent only when what others may see changes, never per scan, and
//! restated after every Ready and Resumed (the server drops a socket's
//! activity when it closes). Sharing off or invisible shows nothing, so it
//! sends `null` once and then nothing. Op 43 goes only to a server whose
//! `PRESENCE_LIST` carries `game_activity`: one that predates it answers an
//! unknown op with an Error frame.

use yap_protocol::game_activity::{ActivityUpdate, GameActivity};
use yap_protocol::payload::PresenceList;
use yap_protocol::GatewayOp;

use super::{frame, Effects};
use crate::store::state::{live_status, State};
use crate::store::view::{self, ViewUpdate};

impl State {
    /// Local detection's latest (already without hidden games).
    pub(super) fn set_game_activity(&mut self, activity: Option<GameActivity>) {
        self.activity.own = activity.filter(GameActivity::is_valid);
    }

    /// Our activity as others may see it: none while not sharing or
    /// invisible (`share_activity` absent means on, as on the web).
    fn activity_shown(&self) -> Option<GameActivity> {
        let me = self.me()?;
        let sharing = me.share_activity != Some(false);
        let invisible = matches!(me.status.as_deref(), Some("invisible" | "offline"));
        if !sharing || invisible {
            return None;
        }
        self.activity.own.clone()
    }

    /// After every input: our row and account line follow what we show,
    /// and the gateway hears a change (once per change).
    pub(super) fn sync_activity(&mut self, fx: &mut Effects) {
        let Some(me_id) = self.me().map(|me| me.id.clone()) else {
            return;
        };
        let shown = self.activity_shown();
        if self.presence.games.get(&me_id) != shown.as_ref() {
            match &shown {
                Some(game) => self.presence.games.insert(me_id, game.clone()),
                None => self.presence.games.remove(&me_id),
            };
            fx.view(ViewUpdate::MyGame {
                name: shown.as_ref().map(|g| g.name.clone()).unwrap_or_default(),
                icon: shown
                    .as_ref()
                    .and_then(|g| g.icon_url.as_deref())
                    .map(|icon| view::absolute_url(&self.api_base, Some(icon)))
                    .unwrap_or_default(),
            });
            self.sync_dms(fx);
            self.sync_members(fx);
            self.emit_profile(fx);
        }
        let ready = self.session.as_ref().is_some_and(|s| s.gateway.ready);
        let activity = &mut self.activity;
        if !(activity.enabled && activity.server && ready) {
            return;
        }
        if activity.sent.as_ref() == Some(&shown) || (!activity.used && shown.is_none()) {
            return;
        }
        activity.sent = Some(shown.clone());
        activity.used = true;
        let update = ActivityUpdate {
            game_activity: shown,
        };
        if let Some(frame) = frame(GatewayOp::ActivityUpdate, &update) {
            fx.push(crate::store::Effect::GatewaySend(frame));
        }
    }

    /// A new socket (Ready or Resumed) or none: nothing was told to it yet.
    pub(super) fn activity_socket_changed(&mut self) {
        self.activity.sent = None;
    }

    /// Sign-out: nothing of this account's activity carries over.
    pub(super) fn clear_activity(&mut self) {
        let own = self.activity.own.take();
        self.activity = crate::store::state::ActivityState {
            own,
            enabled: self.activity.enabled,
            ..Default::default()
        };
    }

    /// One `PRESENCE_UPDATE`: offline (or invisible) clears what they play;
    /// otherwise a present `game_activity` replaces it and an absent one
    /// leaves it. Ours is what we report.
    pub(super) fn apply_presence_game(
        &mut self,
        user_id: &str,
        status: &str,
        game: Option<Option<GameActivity>>,
    ) {
        if self.me().is_some_and(|me| me.id == user_id) {
            return;
        }
        if game.is_some() {
            self.activity.server = true;
        }
        if live_status(Some(status)).is_none() {
            self.presence.games.remove(user_id);
            return;
        }
        match game {
            Some(Some(game)) => {
                self.presence.games.insert(user_id.to_owned(), game);
            }
            Some(None) => {
                self.presence.games.remove(user_id);
            }
            None => {}
        }
    }

    /// A `PRESENCE_LIST`: the activities of everyone online (an entry without
    /// the field keeps what was known), nobody else's. Whether its entries
    /// carry the field says whether the server takes op 43.
    pub(super) fn apply_presence_list_games(&mut self, list: &PresenceList) {
        let me = self.me().map(|me| me.id.clone()).unwrap_or_default();
        let mine = self.presence.games.remove(&me);
        let mut next = std::collections::HashMap::new();
        match &list.users {
            Some(users) => {
                if let Some(first) = users.first() {
                    self.activity.server = first.game_activity.is_some();
                }
                for user in users {
                    let status = user.status.as_deref().unwrap_or("online");
                    if user.user_id == me || live_status(Some(status)).is_none() {
                        continue;
                    }
                    let game = match &user.game_activity {
                        Some(game) => game.clone(),
                        None => self.presence.games.get(&user.user_id).cloned(),
                    };
                    if let Some(game) = game {
                        next.insert(user.user_id.clone(), game);
                    }
                }
            }
            // A server that sends only ids predates game activity.
            None => self.activity.server = false,
        }
        if let Some(mine) = mine {
            next.insert(me, mine);
        }
        self.presence.games = next;
    }
}
