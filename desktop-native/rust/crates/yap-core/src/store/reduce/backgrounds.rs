//! Camera backgrounds (`CameraCard.tsx`): the list once, an upload first in
//! the list, a delete at once (put back on a refusal), and the chosen
//! picture fetched for the camera effect.

use super::Effects;
use crate::api::ApiError;
use crate::store::backgrounds::{
    absolute, BackgroundsCommand, BackgroundsOp, BackgroundsRequest, BackgroundsResponse,
    BackgroundsView,
};
use crate::store::effect::Effect;
use crate::store::state::State;
use crate::store::view::ViewUpdate;
use crate::store::{ApiRequest, ApiResponse, RequestId};

impl State {
    pub(super) fn on_backgrounds(&mut self, fx: &mut Effects, command: BackgroundsCommand) {
        match command {
            BackgroundsCommand::Load => self.load_backgrounds(fx),
            BackgroundsCommand::Upload { path, content_type } => {
                if self.backgrounds.uploading.is_some() {
                    return;
                }
                let Some(token) = self.session.as_ref().map(|s| s.token.clone()) else {
                    self.backgrounds.error = "Sign in to upload backgrounds.".into();
                    self.sync_backgrounds(fx);
                    return;
                };
                let id = RequestId(self.next_id());
                fx.push(Effect::BackgroundUpload {
                    id,
                    token,
                    path,
                    content_type,
                });
                self.pending.backgrounds.insert(id, BackgroundsOp::Upload);
                self.backgrounds.uploading = Some(id);
                self.backgrounds.error.clear();
            }
            BackgroundsCommand::Delete(target) => {
                let previous = self.backgrounds.items.clone();
                self.backgrounds.items.retain(|a| a.id != target);
                if self.backgrounds.items.len() == previous.len() {
                    return;
                }
                self.backgrounds.ready.remove(&target);
                match self.http(
                    fx,
                    ApiRequest::Backgrounds(BackgroundsRequest::Delete(target)),
                ) {
                    Some(id) => {
                        self.pending
                            .backgrounds
                            .insert(id, BackgroundsOp::Delete { previous });
                    }
                    None => self.backgrounds.items = previous,
                }
            }
            BackgroundsCommand::Fetch { id, dest } => {
                if self.backgrounds.ready.get(&id) == Some(&dest)
                    || self.backgrounds.fetching.contains(&id)
                {
                    return;
                }
                let Some(asset) = self.backgrounds.items.iter().find(|a| a.id == id) else {
                    return;
                };
                let Some(token) = self.session.as_ref().map(|s| s.token.clone()) else {
                    return;
                };
                let url = absolute(&asset.url, &crate::config::get().api_base);
                let request = RequestId(self.next_id());
                fx.push(Effect::Download {
                    id: request,
                    token,
                    url,
                    dest: dest.clone(),
                });
                self.backgrounds.fetching.insert(id.clone());
                self.pending
                    .backgrounds
                    .insert(request, BackgroundsOp::Fetch { id, dest });
            }
            BackgroundsCommand::ClearError => self.backgrounds.error.clear(),
        }
        self.sync_backgrounds(fx);
    }

    fn load_backgrounds(&mut self, fx: &mut Effects) {
        if self.backgrounds.loaded
            || self.backgrounds.loading.is_some()
            || self.signed_in().is_none()
        {
            return;
        }
        let id = self.http(fx, ApiRequest::Backgrounds(BackgroundsRequest::List));
        if let Some(id) = id {
            self.pending.backgrounds.insert(id, BackgroundsOp::List);
        }
        self.backgrounds.loading = id;
    }

    pub(super) fn route_backgrounds_http(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        answer: &mut Option<Result<ApiResponse, ApiError>>,
    ) -> bool {
        let Some(op) = self.pending.backgrounds.remove(&id) else {
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
        match (op, result) {
            (BackgroundsOp::List, result) => {
                if self.backgrounds.loading == Some(id) {
                    self.backgrounds.loading = None;
                    match result {
                        Ok(ApiResponse::Backgrounds(BackgroundsResponse::List(items))) => {
                            self.backgrounds.items = items;
                            self.backgrounds.loaded = true;
                        }
                        Err(error) => {
                            self.backgrounds.error =
                                format!("Couldn't load your saved backgrounds ({error}).");
                        }
                        Ok(_) => {}
                    }
                }
            }
            (BackgroundsOp::Upload, result) => {
                self.backgrounds.uploading = None;
                match result {
                    Ok(ApiResponse::Backgrounds(BackgroundsResponse::Uploaded(asset))) => {
                        self.backgrounds.items.retain(|a| a.id != asset.id);
                        self.backgrounds.items.insert(0, asset);
                    }
                    Err(error) => {
                        self.backgrounds.error = format!("Couldn't upload that image ({error}).");
                    }
                    Ok(_) => {}
                }
            }
            (BackgroundsOp::Delete { previous }, result) => {
                if let Err(error) = result {
                    self.backgrounds.items = previous;
                    self.backgrounds.error = format!("Couldn't remove that background ({error}).");
                }
            }
            (BackgroundsOp::Fetch { id, dest }, result) => {
                self.backgrounds.fetching.remove(&id);
                match result {
                    Ok(_) => {
                        self.backgrounds.ready.insert(id, dest);
                    }
                    Err(error) => {
                        self.backgrounds.error =
                            format!("Couldn't download that background ({error}).");
                    }
                }
            }
        }
        self.sync_backgrounds(fx);
        true
    }

    fn sync_backgrounds(&mut self, fx: &mut Effects) {
        let base = &crate::config::get().api_base;
        let view = BackgroundsView {
            items: self
                .backgrounds
                .items
                .iter()
                .map(|a| {
                    let mut a = a.clone();
                    a.url = absolute(&a.url, base);
                    a
                })
                .collect(),
            loading: self.backgrounds.loading.is_some(),
            uploading: self.backgrounds.uploading.is_some(),
            error: self.backgrounds.error.clone(),
            ready: self.backgrounds.ready.clone(),
        };
        fx.view(ViewUpdate::CameraBackgrounds(Box::new(view)));
    }
}
