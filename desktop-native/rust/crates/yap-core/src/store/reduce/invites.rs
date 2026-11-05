//! Create a server (CreateServerModal.tsx), the invite landing
//! (InviteClient.tsx) and the links that lead into the app: each call as
//! the web makes it, with its copy. The web's follow-ups: a created server
//! is added and selected (`ADD_SERVER`, `SET_ACTIVE_SERVER`); a joined one
//! is added as `USER_SERVERS_UPDATE` `upsert` would, then opened from the
//! landing.

use yap_protocol::rest::Server;

use super::Effects;
use crate::api::ApiError;
use crate::links::Link;
use crate::settings::describe_api_error;
use crate::store::invites::{
    member_count, InviteOp, InviteResponse, InviteView, Landing, LandingStep, LandingView,
};
use crate::store::state::{Phase, State};
use crate::store::view::absolute_url;
use crate::store::{
    AdminResponse, ApiRequest, ApiResponse, CacheWrite, Effect, InviteCommand, InviteRequest,
    ProfileAsset, RequestId,
};

/// CreateServerModal.tsx `handleCreate` with no name.
const NAME_MISSING: &str = "Give your server a name.";

/// `createServer` answers `null` on any failure, so the web words every one
/// as `describeApiError(null, "create the server")`.
fn create_failed() -> String {
    describe_api_error(&ApiError::Network(String::new()), "create the server")
}

fn status(error: &ApiError) -> Option<u16> {
    match error {
        ApiError::Http { status, .. } => Some(*status),
        _ => None,
    }
}

fn invite_response(result: Result<ApiResponse, ApiError>) -> Result<InviteResponse, ApiError> {
    match result? {
        ApiResponse::Invites(response) => Ok(response),
        _ => Err(ApiError::Decode("unexpected response".into())),
    }
}

impl State {
    /// Signed in with the shell up: links act now, else they wait.
    fn links_ready(&self) -> bool {
        self.phase == Phase::Ready && self.signed_in().is_some()
    }

    fn invite_http(
        &mut self,
        fx: &mut Effects,
        request: InviteRequest,
        op: InviteOp,
    ) -> Option<RequestId> {
        let id = self.http(fx, ApiRequest::Invites(request))?;
        self.invites.pending.insert(id, op);
        Some(id)
    }

    pub(super) fn on_invites(&mut self, fx: &mut Effects, command: InviteCommand) {
        match command {
            InviteCommand::OpenLink(link) => {
                if self.links_ready() {
                    self.follow_link(fx, link);
                } else {
                    // The latest wins, as a second launch's link would.
                    self.invites.pending_link = Some(link);
                }
            }
            _ if self.signed_in().is_none() => {}
            InviteCommand::StartCreate => {
                if !self.invites.create.busy {
                    self.invites.create.error.clear();
                }
                self.invites.create_name_error.clear();
            }
            InviteCommand::CreateServer { name, icon } => self.create_server(fx, &name, icon),
            InviteCommand::Accept => self.accept_invite(fx),
            InviteCommand::Retry => {
                let Some(landing) = &self.invites.landing else {
                    return;
                };
                if landing.step != LandingStep::Error {
                    return;
                }
                if landing.invite.is_some() {
                    self.accept_invite(fx);
                } else {
                    let code = landing.code.clone();
                    self.load_preview(fx, code);
                }
            }
            InviteCommand::CloseLanding => {
                if !matches!(
                    self.invites.landing.as_ref().map(|l| &l.step),
                    Some(LandingStep::Joining)
                ) {
                    self.invites.landing = None;
                }
            }
            InviteCommand::OpenServer(server_id) => {
                self.invites.landing = None;
                if self.servers.iter().any(|s| s.id == server_id) {
                    self.select_server(fx, server_id);
                }
            }
        }
    }

    fn follow_link(&mut self, fx: &mut Effects, link: Link) {
        match link {
            Link::Invite(code) => {
                self.invites.landing_seq += 1;
                self.load_preview(fx, code);
            }
            Link::Channel {
                server_id,
                channel_id,
                message_id,
            } => {
                // `@me` is a DM: the channel finds its own way there.
                let hint = (server_id != "@me").then_some(server_id);
                self.open_channel(fx, &channel_id, hint);
                if let Some(message_id) = message_id {
                    self.invites.jump = Some((channel_id, message_id));
                    self.invites.jump_seq += 1;
                }
            }
        }
    }

    /// The landing on `code`, loading its preview.
    fn load_preview(&mut self, fx: &mut Effects, code: String) {
        let request = InviteRequest::Preview { code: code.clone() };
        let id = self.invite_http(fx, request, InviteOp::Preview);
        self.invites.landing = Some(Landing {
            code,
            step: if id.is_some() {
                LandingStep::Loading
            } else {
                LandingStep::Error
            },
            invite: None,
            request: id,
        });
    }

    fn accept_invite(&mut self, fx: &mut Effects) {
        let Some(landing) = &self.invites.landing else {
            return;
        };
        let ready = matches!(landing.step, LandingStep::Preview | LandingStep::Error);
        if !ready || landing.invite.is_none() {
            return;
        }
        let request = InviteRequest::Join {
            code: landing.code.clone(),
        };
        let id = self.invite_http(fx, request, InviteOp::Join);
        if let Some(landing) = &mut self.invites.landing {
            landing.request = id;
            landing.step = if id.is_some() {
                LandingStep::Joining
            } else {
                LandingStep::Error
            };
        }
    }

    fn create_server(&mut self, fx: &mut Effects, name: &str, icon: Option<(String, String)>) {
        if self.invites.create.busy {
            return;
        }
        let name = name.trim().to_owned();
        if name.is_empty() {
            self.invites.create_name_error = NAME_MISSING.into();
            return;
        }
        self.invites.create_name_error.clear();
        self.invites.create.error.clear();
        match icon {
            Some((path, content_type)) => {
                let Some(token) = self.session.as_ref().map(|s| s.token.clone()) else {
                    return;
                };
                self.invites.create.busy = true;
                let id = RequestId(self.next_id());
                fx.push(Effect::ProfileUpload {
                    id,
                    token,
                    asset: ProfileAsset::ServerIcon,
                    path,
                    content_type,
                });
                self.invites.pending.insert(id, InviteOp::Icon { name });
                self.invites.create_request = Some(id);
            }
            None => {
                self.invites.create.busy = true;
                self.send_create(fx, name, None);
            }
        }
    }

    fn send_create(&mut self, fx: &mut Effects, name: String, icon_url: Option<String>) {
        let request = InviteRequest::CreateServer { name, icon_url };
        self.invites.create_request = self.invite_http(fx, request, InviteOp::Create);
        if self.invites.create_request.is_none() {
            self.invites.create.busy = false;
            self.invites.create.error = create_failed();
        }
    }

    /// A server the user is in now: added to the rail (or updated there).
    fn upsert_server(&mut self, fx: &mut Effects, server: Server) {
        match self.servers.iter_mut().find(|s| s.id == server.id) {
            Some(known) => *known = server,
            None => self.servers.push(server),
        }
        self.sync_servers(fx);
        self.persist(fx, CacheWrite::Servers(self.servers.clone()));
    }

    pub(super) fn route_invites_http(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        result: &mut Option<Result<ApiResponse, ApiError>>,
    ) -> bool {
        let Some(op) = self.invites.pending.remove(&id) else {
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
        match op {
            InviteOp::Icon { name } => self.on_icon(fx, id, name, result),
            InviteOp::Create => self.on_created(fx, id, invite_response(result)),
            InviteOp::Preview => self.on_preview(id, invite_response(result)),
            InviteOp::Join => self.on_joined(fx, id, invite_response(result)),
        }
        true
    }

    fn on_icon(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        name: String,
        result: Result<ApiResponse, ApiError>,
    ) {
        if self.invites.create_request != Some(id) {
            return;
        }
        match result {
            Ok(ApiResponse::Admin(AdminResponse::IconUrl(url))) => {
                self.send_create(fx, name, Some(url));
            }
            other => {
                let error = match other {
                    Err(error) => error,
                    Ok(_) => ApiError::Decode("no icon URL".into()),
                };
                self.invites.create_request = None;
                self.invites.create.busy = false;
                self.invites.create.error = describe_api_error(&error, "upload that icon");
            }
        }
    }

    fn on_created(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        result: Result<InviteResponse, ApiError>,
    ) {
        if self.invites.create_request != Some(id) {
            return;
        }
        self.invites.create_request = None;
        self.invites.create.busy = false;
        match result {
            Ok(InviteResponse::Created(Some(server))) => {
                let server_id = server.id.clone();
                self.upsert_server(fx, *server);
                self.select_server(fx, server_id);
                self.invites.create.done += 1;
            }
            _ => self.invites.create.error = create_failed(),
        }
    }

    fn on_preview(&mut self, id: RequestId, result: Result<InviteResponse, ApiError>) {
        let Some(landing) = self
            .invites
            .landing
            .as_mut()
            .filter(|l| l.request == Some(id))
        else {
            return;
        };
        landing.request = None;
        match result {
            Ok(InviteResponse::Preview(invite)) => {
                landing.invite = Some(*invite);
                landing.step = LandingStep::Preview;
            }
            Err(error) => {
                landing.step = match status(&error) {
                    Some(410) => LandingStep::Invalid { expired: true },
                    Some(404) => LandingStep::Invalid { expired: false },
                    _ => LandingStep::Error,
                };
            }
            Ok(_) => landing.step = LandingStep::Error,
        }
    }

    fn on_joined(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        result: Result<InviteResponse, ApiError>,
    ) {
        let Some(landing) = self
            .invites
            .landing
            .as_mut()
            .filter(|l| l.request == Some(id))
        else {
            return;
        };
        landing.request = None;
        let fallback = landing
            .invite
            .as_ref()
            .map(|i| (i.server.id.clone(), i.server.name.clone()))
            .unwrap_or_default();
        let joined = match result {
            Ok(InviteResponse::Joined(joined)) => *joined,
            Err(error) => {
                landing.step = match status(&error) {
                    Some(410) => LandingStep::Invalid { expired: true },
                    Some(404) => LandingStep::Invalid { expired: false },
                    _ => LandingStep::Error,
                };
                return;
            }
            Ok(_) => {
                landing.step = LandingStep::Error;
                return;
            }
        };
        let (server_id, name) = joined
            .server
            .as_ref()
            .map(|s| (s.id.clone(), s.name.clone()))
            .unwrap_or(fallback);
        if joined.already_member {
            landing.step = LandingStep::Member { server_id, name };
            return;
        }
        let Some(server) = joined.server else {
            landing.step = LandingStep::Error;
            return;
        };
        landing.step = LandingStep::Joined { server_id, name };
        self.upsert_server(fx, server);
    }

    pub(super) fn clear_invites(&mut self) {
        let pending_link = self.invites.pending_link.take();
        let shown = std::mem::take(&mut self.invites.shown);
        self.invites = Default::default();
        // A link that came while signed out waits for the next sign-in.
        self.invites.pending_link = pending_link;
        self.invites.shown = shown;
    }

    fn invite_view(&self) -> InviteView {
        let s = &self.invites;
        let (jump_channel_id, jump_message_id) = s.jump.clone().unwrap_or_default();
        InviteView {
            create: s.create.clone(),
            create_name_error: s.create_name_error.clone(),
            landing: s.landing.as_ref().map(|l| self.landing_view(l)),
            landing_seq: s.landing_seq,
            jump_channel_id,
            jump_message_id,
            jump_seq: s.jump_seq,
        }
    }

    fn landing_view(&self, landing: &Landing) -> LandingView {
        let (step, expired) = match &landing.step {
            LandingStep::Loading => ("loading", false),
            LandingStep::Preview => ("preview", false),
            LandingStep::Joining => ("joining", false),
            LandingStep::Joined { .. } => ("joined", false),
            LandingStep::Member { .. } => ("member", false),
            LandingStep::Invalid { expired } => ("invalid", *expired),
            LandingStep::Error => ("error", false),
        };
        let mut view = LandingView {
            code: landing.code.clone(),
            step: step.into(),
            expired,
            has_invite: landing.invite.is_some(),
            ..LandingView::default()
        };
        if let Some(invite) = &landing.invite {
            let inviter = &invite.inviter;
            view.server_id = invite.server.id.clone();
            view.server_name = invite.server.name.clone();
            view.server_icon = match invite.server.icon_url.as_deref() {
                Some(url) if !url.is_empty() => absolute_url(&self.api_base, Some(url)),
                _ => String::new(),
            };
            view.members = member_count(invite.server.member_count);
            view.inviter = inviter
                .display_name
                .clone()
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| inviter.username.clone());
        }
        if let LandingStep::Joined { server_id, name } | LandingStep::Member { server_id, name } =
            &landing.step
        {
            view.server_id = server_id.clone();
            view.server_name = name.clone();
        }
        // `name.charAt(0).toUpperCase()`.
        view.server_initial = view
            .server_name
            .chars()
            .next()
            .map(|c| c.to_uppercase().collect())
            .unwrap_or_default();
        view
    }

    /// After every input: a waiting link acts once the shell is ready, and
    /// the view goes out when it changed.
    pub(super) fn sync_invites(&mut self, fx: &mut Effects) {
        if self.links_ready() {
            if let Some(link) = self.invites.pending_link.take() {
                self.follow_link(fx, link);
            }
        }
        let view = self.invite_view();
        if view != self.invites.shown {
            self.invites.shown = view.clone();
            fx.view(crate::store::ViewUpdate::Invites(Box::new(view)));
        }
    }
}
