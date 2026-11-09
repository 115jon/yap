//! TikTok and Instagram embeds refresh their signed media through
//! `/api/tiktok-video` and `/api/instagram-video` (LinkEmbed.tsx,
//! embed-hydration-cache.ts). A card asks when it shows; the store keeps one
//! answer per link for the session and re-asks once it is stale: a good
//! answer after 30 minutes (signed URLs expire), a confirmed-gone post
//! after 10, a failure after 30 seconds.

use std::collections::HashMap;

use serde_json::Value;

use super::{Method, RequestId};
use crate::api::ApiError;

/// Which lookup an embed uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    TikTok,
    Instagram,
}

impl Provider {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "tiktok" => Some(Self::TikTok),
            "instagram" => Some(Self::Instagram),
            _ => None,
        }
    }

    fn route(self) -> &'static str {
        match self {
            Self::TikTok => "/api/tiktok-video",
            Self::Instagram => "/api/instagram-video",
        }
    }
}

/// The map key of a link's answer.
pub fn key(provider: Provider, url: &str) -> String {
    let route = provider.route();
    format!("{route}|{url}")
}

/// What the lookup said.
#[derive(Debug, Clone, PartialEq)]
pub enum Hydration {
    Pending,
    Ready(Value),
    /// The server confirmed the post is gone (404).
    Missing,
    /// Another attempt may help when `transient` (5xx, 429, network).
    Failed {
        transient: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub state: Hydration,
    /// When it was asked or answered (epoch ms, from the UI's clock).
    pub at_ms: i64,
}

pub type Hydrations = HashMap<String, Entry>;

const READY_TTL_MS: i64 = 30 * 60 * 1000;
const MISSING_TTL_MS: i64 = 10 * 60 * 1000;
const FAILED_TTL_MS: i64 = 30 * 1000;

impl Entry {
    /// Still good at `now_ms` (a pending lookup always is).
    pub fn fresh(&self, now_ms: i64) -> bool {
        let ttl = match self.state {
            Hydration::Pending => return true,
            Hydration::Ready(_) => READY_TTL_MS,
            Hydration::Missing => MISSING_TTL_MS,
            Hydration::Failed { .. } => FAILED_TTL_MS,
        };
        now_ms - self.at_ms < ttl
    }
}

/// A card's request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbedsCommand {
    /// A card showed: look it up unless a fresh answer is known.
    Hydrate {
        provider: Provider,
        url: String,
        now_ms: i64,
    },
    /// "Try again" on a failed card.
    Retry {
        provider: Provider,
        url: String,
        now_ms: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbedsRequest {
    pub provider: Provider,
    pub url: String,
}

impl EmbedsRequest {
    pub fn method(&self) -> Method {
        Method::Get
    }

    pub fn path(&self) -> String {
        format!(
            "{}?videoUrl={}",
            self.provider.route(),
            super::view::query_value(&self.url)
        )
    }
}

/// How a failed lookup reads.
pub fn failure(error: &ApiError) -> Hydration {
    match error {
        ApiError::Http { status: 404, .. } => Hydration::Missing,
        ApiError::Http { status, .. } => Hydration::Failed {
            transient: *status == 429 || *status >= 500,
        },
        ApiError::Network(_) => Hydration::Failed { transient: true },
        _ => Hydration::Failed { transient: false },
    }
}

/// The lookups on their way.
pub(super) type Pending = HashMap<RequestId, (Provider, String)>;
