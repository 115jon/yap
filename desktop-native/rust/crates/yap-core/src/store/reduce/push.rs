//! A phone's push token (`/api/push-tokens`, as the Expo app's
//! features/push/push.ts registers it) and its background suspend: the
//! gateway closes a while after the app leaves the screen, when the
//! pushes take over, and comes back with it.

use std::time::Duration;

use super::Effects;
use crate::api::ApiError;
use crate::store::state::{PushRegistration, State};
use crate::store::{ApiRequest, ApiResponse, Effect, HttpReason, Input, RequestId, SessionId};

/// How long a phone app stays connected in the background before its
/// gateway closes: a quick trip to another app keeps the socket.
pub const SUSPEND_GRACE: Duration = Duration::from_secs(60);

impl State {
    pub(super) fn register_push(&mut self, fx: &mut Effects, registration: PushRegistration) {
        let token = registration.token.trim();
        if token.is_empty() {
            return;
        }
        let registration = PushRegistration {
            token: token.to_owned(),
            ..registration
        };
        if self.push.as_ref() == Some(&registration) && self.push_registered {
            return;
        }
        self.push = Some(registration);
        self.push_registered = false;
        self.sync_push(fx);
    }

    /// Registers the token for the signed-in account, once per session (a
    /// failed try again at the next gateway Ready).
    pub(super) fn sync_push(&mut self, fx: &mut Effects) {
        if self.push_registered || self.signed_in().is_none() {
            return;
        }
        let Some(push) = self.push.clone() else {
            return;
        };
        let request = ApiRequest::RegisterPush {
            token: push.token,
            platform: push.platform,
            app_variant: push.app_variant,
            push_format: push.format,
        };
        self.push_request = self.http(fx, request);
        self.push_registered = self.push_request.is_some();
    }

    /// The registration's answer. Refused or unreachable: not registered,
    /// so the next Ready asks again (until then the server may still send
    /// this device a previous account's pushes, which the app drops when
    /// signed out; see MessageNotifier).
    pub(super) fn route_push_http(
        &mut self,
        id: RequestId,
        answer: &mut Option<Result<ApiResponse, ApiError>>,
    ) -> bool {
        if self.push_request != Some(id) {
            return false;
        }
        self.push_request = None;
        if let Some(Err(error)) = answer.take() {
            eprintln!("[push] register failed: {error}");
            self.push_registered = false;
        }
        true
    }

    /// The account stops getting this device's pushes. `keep`: the token
    /// stays for the next account (a sign-out); otherwise it is forgotten.
    pub(super) fn unregister_push(&mut self, fx: &mut Effects, keep: bool) {
        let token = self.push.as_ref().map(|push| push.token.clone());
        if !keep {
            self.push = None;
        }
        self.push_request = None;
        if !std::mem::take(&mut self.push_registered) {
            return;
        }
        if let Some(token) = token {
            self.http_for(fx, ApiRequest::UnregisterPush { token }, HttpReason::SignOut);
        }
    }

    pub(super) fn set_suspended(&mut self, fx: &mut Effects, suspended: bool) {
        if suspended == self.suspended {
            return;
        }
        self.suspended = suspended;
        self.suspend_generation += 1;
        if suspended {
            fx.push(Effect::Later {
                delay: SUSPEND_GRACE,
                input: Box::new(Input::SuspendElapsed {
                    generation: self.suspend_generation,
                }),
            });
        } else if std::mem::take(&mut self.gateway_suspended) {
            self.retry_gateway(fx);
        } else if self
            .signed_in()
            .is_some_and(|(session, _)| session.gateway.was_ready && !session.gateway.ready)
        {
            // Back before the grace with the link down (a phone blocks a
            // background app's network, so its tries failed and the backoff
            // grew): a fresh connection now, its backoff from the start,
            // rather than at the end of a wait of up to 16 s.
            self.retry_gateway(fx);
        }
    }

    /// Still in the background after the grace: the gateway closes, unless
    /// a call needs it or the session is still booting; then it asks again
    /// after another grace.
    pub(super) fn on_suspend_elapsed(&mut self, fx: &mut Effects, generation: u64) {
        if generation != self.suspend_generation || !self.suspended || self.gateway_suspended {
            return;
        }
        if self.voice.is_some() || self.signed_in().is_none() {
            if self.session.is_some() {
                fx.push(Effect::Later {
                    delay: SUSPEND_GRACE,
                    input: Box::new(Input::SuspendElapsed { generation }),
                });
            }
            return;
        }
        // A fresh id: whatever the old socket queued (a late Ready) no
        // longer counts, so the resume always reconnects.
        let id = SessionId(self.next_id());
        if let Some(session) = self.session.as_mut() {
            session.id = id;
            session.gateway.ready = false;
        }
        fx.push(Effect::GatewayDisconnect);
        self.gateway_suspended = true;
        self.activity_socket_changed();
        self.set_connected(fx, false);
    }
}
