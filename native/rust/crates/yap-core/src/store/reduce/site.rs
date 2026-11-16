//! The site announcement ([`crate::store::site`]).

use super::Effects;
use crate::api::ApiError;
use crate::site_config::{self, REFRESH};
use crate::store::site::SiteCommand;
use crate::store::state::State;
use crate::store::{ApiRequest, ApiResponse, Effect, Input, RequestId, ViewUpdate};

impl State {
    pub(super) fn on_site(&mut self, fx: &mut Effects, command: SiteCommand) {
        match command {
            SiteCommand::Dismiss => {
                let Some(shown) = self.site.shown.as_ref() else {
                    return;
                };
                if shown.critical() {
                    return;
                }
                let id = shown.id.clone();
                fx.push(Effect::SaveLocal {
                    file: site_config::DISMISSED_FILE,
                    contents: site_config::dismissed_file(&id),
                });
                self.site.dismissed = Some(id);
            }
            SiteCommand::Dismissed(id) => {
                if !id.is_empty() {
                    self.site.dismissed = Some(id);
                }
            }
        }
    }

    /// The app is on screen: a desktop window shown, a phone app in front.
    fn site_visible(&self) -> bool {
        self.window_shown && !self.suspended
    }

    /// The app came back on screen: the config is asked for again
    /// (the web's `visibilitychange`).
    pub(super) fn site_on_screen(&mut self) {
        self.site.due = true;
    }

    /// The refresh timer ran out; only the newest counts.
    pub(super) fn on_site_refresh(&mut self, generation: u64) {
        if generation == self.site.generation {
            self.site.due = true;
        }
    }

    pub(super) fn route_site_http(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        answer: &mut Option<Result<ApiResponse, ApiError>>,
    ) -> bool {
        if self.site.request != Some(id) {
            return false;
        }
        self.site.request = None;
        match answer.take() {
            Some(Ok(ApiResponse::SiteConfig(announcement))) => {
                self.site.announcement = announcement.map(|a| *a);
            }
            // Offline, or an older server without the route: what showed
            // stays (the web keeps its last answer too).
            Some(_) | None => {}
        }
        self.site.generation += 1;
        fx.push(Effect::Later {
            delay: REFRESH,
            input: Box::new(Input::SiteRefresh {
                generation: self.site.generation,
            }),
        });
        true
    }

    /// Asks when due, and sends the view when it changed. Runs after every
    /// input.
    pub(super) fn sync_site(&mut self, fx: &mut Effects) {
        let account = self.signed_in().map(|(_, me)| me.id.clone());
        match account {
            None => {
                if self.site.account.is_some() {
                    // Signed out: nothing shows, and the next account
                    // asks afresh. The dismissal is the device's.
                    self.site.account = None;
                    self.site.request = None;
                    self.site.announcement = None;
                    self.site.due = false;
                    self.site.generation += 1;
                }
            }
            Some(id) => {
                let fresh = self.site.account.as_ref() != Some(&id);
                if fresh {
                    self.site.account = Some(id);
                    self.site.request = None;
                    self.site.announcement = None;
                    self.site.generation += 1;
                }
                let due = fresh || self.site.due;
                if due && self.site.request.is_none() && self.site_visible() {
                    self.site.due = false;
                    self.site.request = self.http(fx, ApiRequest::SiteConfig);
                } else if fresh {
                    // Hidden at sign-in: asked when the app is on screen.
                    self.site.due = true;
                }
            }
        }
        let site = &self.site;
        let view = site
            .announcement
            .as_ref()
            .filter(|a| a.critical() || site.dismissed.as_deref() != Some(a.id.as_str()))
            .cloned();
        if self.site.shown != view {
            self.site.shown.clone_from(&view);
            fx.view(ViewUpdate::Announcement(view));
        }
    }
}
