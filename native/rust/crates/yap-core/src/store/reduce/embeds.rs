//! TikTok and Instagram lookups (embed-hydration-cache.ts): a card that
//! shows asks; one request per link at a time; the answer re-renders every
//! open row that links it.

use super::Effects;
use crate::api::ApiError;
use crate::store::embeds::{
    failure, key, EmbedsCommand, EmbedsRequest, Entry, Hydration, Provider,
};
use crate::store::state::State;
use crate::store::{ApiRequest, ApiResponse, RequestId};

impl State {
    pub(super) fn on_embeds(&mut self, fx: &mut Effects, command: EmbedsCommand) {
        let (provider, url, now_ms, force) = match command {
            EmbedsCommand::Hydrate {
                provider,
                url,
                now_ms,
            } => (provider, url, now_ms, false),
            EmbedsCommand::Retry {
                provider,
                url,
                now_ms,
            } => (provider, url, now_ms, true),
        };
        if url.is_empty() {
            return;
        }
        let entry_key = key(provider, &url);
        if let Some(entry) = self.hydrations.get(&entry_key) {
            let pending = entry.state == Hydration::Pending;
            if pending || (!force && entry.fresh(now_ms)) {
                return;
            }
        }
        let request = ApiRequest::Embeds(EmbedsRequest {
            provider,
            url: url.clone(),
        });
        let Some(id) = self.http(fx, request) else {
            return;
        };
        self.pending.embeds.insert(id, (provider, url.clone()));
        let entry = Entry {
            state: Hydration::Pending,
            at_ms: now_ms,
        };
        // A retry shows the loading state again (the card had failed).
        let indices = self.embed_rows(&url);
        self.touch_rows(fx, &indices, move |state| {
            state.hydrations.insert(entry_key, entry);
        });
    }

    pub(super) fn route_embeds_http(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        answer: &mut Option<Result<ApiResponse, ApiError>>,
    ) -> bool {
        let Some((provider, url)) = self.pending.embeds.remove(&id) else {
            return false;
        };
        let Some(result) = answer.take() else {
            return true;
        };
        let state = match result {
            Ok(ApiResponse::Embeds(payload)) if payload.is_object() => Hydration::Ready(payload),
            Ok(_) => Hydration::Failed { transient: false },
            Err(error) => failure(&error),
        };
        self.settle_hydration(fx, provider, &url, state);
        true
    }

    fn settle_hydration(
        &mut self,
        fx: &mut Effects,
        provider: Provider,
        url: &str,
        state: Hydration,
    ) {
        let entry_key = key(provider, url);
        let at_ms = self.hydrations.get(&entry_key).map_or(0, |e| e.at_ms);
        let indices = self.embed_rows(url);
        self.touch_rows(fx, &indices, move |s| {
            s.hydrations.insert(entry_key, Entry { state, at_ms });
        });
    }

    /// The open channel's rows that carry an embed of `url`.
    fn embed_rows(&self, url: &str) -> Vec<usize> {
        self.messages
            .as_ref()
            .map(|log| {
                log.iter()
                    .enumerate()
                    .filter(|(_, m)| m.embeds.iter().any(|e| e.url == url))
                    .map(|(i, _)| i)
                    .collect()
            })
            .unwrap_or_default()
    }
}
