//! The site announcement in the store ([`crate::site_config`]): fetched
//! once the account is known, again every [`crate::site_config::REFRESH`]
//! while the app is on screen and when it comes back, and dismissed per
//! announcement on this device.

use super::RequestId;
use crate::site_config::Announcement;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SiteCommand {
    /// The banner's dismiss key: this announcement stays hidden on this
    /// device (a critical one cannot be dismissed).
    Dismiss,
    /// The id dismissed in an earlier run, read from the app folder at
    /// launch.
    Dismissed(String),
}

/// Where the announcement is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SiteState {
    /// The running announcement, as last fetched.
    pub announcement: Option<Announcement>,
    /// The id dismissed on this device.
    pub dismissed: Option<String>,
    /// The fetch in flight.
    pub request: Option<RequestId>,
    /// The account the announcement was fetched for (its user id); a sign
    /// out or another account fetches afresh. Not the session id, which
    /// changes with each gateway reconnect of the same sign-in.
    pub account: Option<String>,
    /// Due again (the refresh timer ran out, or the app came back on
    /// screen): fetched as soon as the app is on screen.
    pub due: bool,
    /// Only the newest refresh timer counts.
    pub generation: u64,
    /// The view last sent.
    pub shown: Option<Announcement>,
}
