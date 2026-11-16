//! Settings › Connections ([`crate::store::connections`]).

use super::Effects;
use crate::api::ApiError;
use crate::connections::{self, ConnectionsRequest, ConnectionsResponse};
use crate::links::ConnectionOutcome;
use crate::settings::describe_api_error;
use crate::store::connections::{
    ConnectionCard, ConnectionsCommand, ConnectionsOp, ConnectionsState, ConnectionsView,
};
use crate::store::state::State;
use crate::store::time::{local_date, parse_rfc3339};
use crate::store::view::query_value;
use crate::store::{ApiRequest, ApiResponse, RequestId, ViewUpdate};

impl State {
    pub(super) fn on_connections(&mut self, fx: &mut Effects, command: ConnectionsCommand) {
        if self.signed_in().is_none() {
            return;
        }
        match command {
            ConnectionsCommand::Load => self.load_connections(fx),
            ConnectionsCommand::Connect { provider, app } => {
                if !connections::is_provider(&provider) || self.connections.busy.is_some() {
                    return;
                }
                self.connections.action_error.clear();
                self.connections.notice = None;
                let request = ConnectionsRequest::Start {
                    provider: provider.clone(),
                    app,
                };
                self.connections_http(fx, request, ConnectionsOp::Start(provider), "connect");
            }
            ConnectionsCommand::Unlink(provider) => {
                if !self.is_linked(&provider) || self.connections.busy.is_some() {
                    return;
                }
                self.connections.action_error.clear();
                let request = ConnectionsRequest::Unlink {
                    provider: provider.clone(),
                };
                self.connections_http(fx, request, ConnectionsOp::Unlink(provider), "unlink");
            }
            ConnectionsCommand::SetShown { provider, show } => {
                if self.connections.busy.is_some() {
                    return;
                }
                let Some(previous) = self.set_shown_locally(&provider, show) else {
                    return;
                };
                self.connections.action_error.clear();
                let request = ConnectionsRequest::SetShown {
                    provider: provider.clone(),
                    show,
                };
                let op = ConnectionsOp::SetShown { provider, previous };
                self.connections_http(fx, request, op, "show");
            }
            ConnectionsCommand::DismissNotice => self.connections.notice = None,
        }
    }

    /// The browser handed a link's outcome back: claim a verified account,
    /// or say how it ended; Connections shows either way.
    pub(super) fn on_connection_link(
        &mut self,
        fx: &mut Effects,
        provider: String,
        outcome: ConnectionOutcome,
    ) {
        self.connections.show_seq += 1;
        self.connections.action_error.clear();
        match outcome {
            ConnectionOutcome::Code(code) => {
                // A claim waits for nothing else in flight: the code is
                // only good for a few minutes.
                let request = ConnectionsRequest::Complete {
                    provider: provider.clone(),
                    code,
                };
                self.connections.busy = None;
                self.connections_http(fx, request, ConnectionsOp::Complete(provider), "complete");
            }
            ConnectionOutcome::Status(status) => {
                self.set_notice(&provider, &status);
                self.load_connections(fx);
            }
        }
    }

    fn set_notice(&mut self, provider: &str, status: &str) {
        let (tone, text) = connections::describe_result(provider, status);
        self.connections.notice = Some((tone, text));
    }

    fn is_linked(&self, provider: &str) -> bool {
        self.connections
            .list
            .as_ref()
            .is_some_and(|list| list.iter().any(|c| c.provider == provider))
    }

    /// Flips the switch at once; the old value, or `None` when not linked.
    fn set_shown_locally(&mut self, provider: &str, show: bool) -> Option<bool> {
        let connection = self
            .connections
            .list
            .as_mut()?
            .iter_mut()
            .find(|c| c.provider == provider)?;
        let previous = connection.show_on_profile;
        connection.show_on_profile = show;
        Some(previous)
    }

    fn load_connections(&mut self, fx: &mut Effects) {
        if self.connections.loading.is_some() {
            return;
        }
        self.connections.load_error.clear();
        let id = self.http(fx, ApiRequest::Connections(ConnectionsRequest::List));
        if let Some(id) = id {
            self.connections.pending.insert(id, ConnectionsOp::List);
        }
        self.connections.loading = id;
    }

    fn connections_http(
        &mut self,
        fx: &mut Effects,
        request: ConnectionsRequest,
        op: ConnectionsOp,
        action: &'static str,
    ) {
        let provider = match &op {
            ConnectionsOp::Start(p)
            | ConnectionsOp::Complete(p)
            | ConnectionsOp::Unlink(p)
            | ConnectionsOp::SetShown { provider: p, .. } => p.clone(),
            ConnectionsOp::List => String::new(),
        };
        if let Some(id) = self.http(fx, ApiRequest::Connections(request)) {
            self.connections.pending.insert(id, op);
            self.connections.busy = Some((provider, action));
        }
    }

    pub(super) fn route_connections_http(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        answer: &mut Option<Result<ApiResponse, ApiError>>,
    ) -> bool {
        let Some(op) = self.connections.pending.remove(&id) else {
            return false;
        };
        let Some(result) = answer.take() else {
            return true;
        };
        if let Err(error) = &result {
            if error.is_unauthorized() {
                self.fail(fx, error);
                return true;
            }
        }
        let result = result.and_then(|response| match response {
            ApiResponse::Connections(response) => Ok(response),
            _ => Err(ApiError::Decode("unexpected response".into())),
        });
        if !matches!(op, ConnectionsOp::List) {
            self.connections.busy = None;
        }
        match (op, result) {
            (ConnectionsOp::List, result) => {
                if self.connections.loading == Some(id) {
                    self.connections.loading = None;
                    match result {
                        Ok(ConnectionsResponse::List(list)) => self.connections.list = Some(list),
                        Ok(_) => {}
                        Err(error) => {
                            self.connections.load_error =
                                describe_api_error(&error, "load your connections");
                        }
                    }
                }
            }
            (ConnectionsOp::Start(_), Ok(ConnectionsResponse::Started(url))) => {
                self.connections.open_url = url;
                self.connections.open_seq += 1;
            }
            (ConnectionsOp::Start(provider), result) => {
                let label = connections::label(&provider);
                self.connections.action_error = match result {
                    Err(error) => describe_api_error(&error, &format!("start connecting {label}")),
                    Ok(_) => format!("Couldn't start connecting {label}. Try again."),
                };
            }
            (ConnectionsOp::Complete(provider), Ok(ConnectionsResponse::Completed(status))) => {
                self.set_notice(&provider, &status);
                self.load_connections(fx);
            }
            (ConnectionsOp::Complete(provider), Ok(_) | Err(_)) => {
                let label = connections::label(&provider);
                self.connections.notice = Some((
                    "error",
                    format!("Couldn't finish connecting {label}. Start again from Connections."),
                ));
                self.load_connections(fx);
            }
            (ConnectionsOp::Unlink(provider), Ok(_)) => {
                if let Some(list) = self.connections.list.as_mut() {
                    list.retain(|c| c.provider != provider);
                }
            }
            (ConnectionsOp::Unlink(provider), Err(error)) => {
                let label = connections::label(&provider);
                self.connections.action_error =
                    describe_api_error(&error, &format!("unlink {label}"));
            }
            (ConnectionsOp::SetShown { .. }, Ok(_)) => {}
            (ConnectionsOp::SetShown { provider, previous }, Err(error)) => {
                self.set_shown_locally(&provider, previous);
                self.connections.action_error = describe_api_error(&error, "save this setting");
            }
        }
        true
    }

    /// Forgets the list (a session ended); the view follows.
    pub(super) fn reset_connections(&mut self) {
        let shown = std::mem::take(&mut self.connections.shown);
        let open_seq = self.connections.open_seq;
        let show_seq = self.connections.show_seq;
        self.connections = ConnectionsState {
            shown,
            open_seq,
            show_seq,
            ..ConnectionsState::default()
        };
    }

    pub(super) fn sync_connections(&mut self, fx: &mut Effects) {
        if self.signed_in().is_none() {
            // Signed out: nothing to draw (and nothing sent before a
            // session ever began).
            if self.connections.list.is_some() || self.connections.shown.loaded {
                self.reset_connections();
            }
            if self.connections.shown != ConnectionsView::default() {
                let view = ConnectionsView {
                    open_seq: self.connections.open_seq,
                    show_seq: self.connections.show_seq,
                    ..ConnectionsView::default()
                };
                if self.connections.shown != view {
                    self.connections.shown = view.clone();
                    fx.view(ViewUpdate::Connections(Box::new(view)));
                }
            }
            return;
        }
        let state = &self.connections;
        let cards = connections::PROVIDERS
            .iter()
            .map(|&provider| {
                let linked = state
                    .list
                    .as_ref()
                    .and_then(|list| list.iter().find(|c| c.provider == provider));
                ConnectionCard {
                    provider: provider.to_owned(),
                    label: connections::label(provider).to_owned(),
                    blurb: connections::blurb(provider).to_owned(),
                    linked: linked.is_some(),
                    display_name: linked.map(|c| c.display_name.clone()).unwrap_or_default(),
                    profile_url: linked
                        .and_then(|c| c.profile_url.clone())
                        .filter(|u| u.starts_with("https://"))
                        .unwrap_or_default(),
                    // Through Yap's media proxy, as the web shows it, so
                    // opening Connections doesn't call Steam's CDN.
                    avatar_url: linked
                        .and_then(|c| c.avatar_url.as_deref())
                        .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
                        .map(|u| {
                            format!("{}/api/proxy-media?url={}", self.api_base, query_value(u))
                        })
                        .unwrap_or_default(),
                    show_on_profile: linked.is_some_and(|c| c.show_on_profile),
                    verified: linked
                        .and_then(|c| parse_rfc3339(&c.verified_at))
                        .map(|ms| local_date(ms, self.local_offset, false))
                        .unwrap_or_default(),
                }
            })
            .collect();
        let view = ConnectionsView {
            loaded: state.list.is_some(),
            loading: state.loading.is_some(),
            load_error: state.load_error.clone(),
            cards,
            busy: state
                .busy
                .as_ref()
                .map(|(provider, action)| format!("{action}:{provider}"))
                .unwrap_or_default(),
            action_error: state.action_error.clone(),
            notice_tone: state
                .notice
                .as_ref()
                .map(|n| n.0.to_owned())
                .unwrap_or_default(),
            notice: state
                .notice
                .as_ref()
                .map(|n| n.1.clone())
                .unwrap_or_default(),
            open_url: state.open_url.clone(),
            open_seq: state.open_seq,
            show_seq: state.show_seq,
        };
        if self.connections.shown != view {
            self.connections.shown = view.clone();
            fx.view(ViewUpdate::Connections(Box::new(view)));
        }
    }
}
