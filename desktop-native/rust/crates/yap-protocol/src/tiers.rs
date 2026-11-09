//! Plans and badges as the API sends them (`src/lib/entitlements.ts`
//! `parseEntitlement`, `src/lib/badges.ts` `parseUserBadges`).
//!
//! `GET /api/users/me` carries the full entitlement and the badges;
//! `GET /api/users/:id/profile` carries `entitlement: { tier }` and the
//! badges. Both are read leniently: an entitlement that does not decode is
//! free (no plan), and a badge that does not decode is skipped. Which badge
//! ids are known, their order and the perks are the core's
//! (`yap_core::badges`, `yap_core::entitlements`).

use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `TIERS`. No row in `user_entitlements` means free.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    #[default]
    Free,
    Loud,
    Founder,
}

impl Tier {
    pub fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "free" => Self::Free,
            "loud" => Self::Loud,
            "founder" => Self::Founder,
            _ => return None,
        })
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Loud => "loud",
            Self::Founder => "founder",
        }
    }
}

/// `ENTITLEMENT_SOURCES`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntitlementSource {
    Grant,
    Stripe,
    Admin,
}

/// A user's plan. Billing ids never leave the server.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entitlement {
    pub tier: Tier,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<EntitlementSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    /// None for founders and admin grants: they do not run out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

impl Entitlement {
    /// `parseEntitlement`: anything unreadable is `None` (free). A known
    /// tier keeps a known source and non-empty dates; the rest is dropped.
    pub fn parse(value: &Value) -> Option<Self> {
        let raw = value.as_object()?;
        let tier = Tier::from_key(raw.get("tier")?.as_str()?)?;
        let text = |key: &str| {
            raw.get(key)
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };
        let source = raw
            .get("source")
            .and_then(Value::as_str)
            .and_then(|s| match s {
                "grant" => Some(EntitlementSource::Grant),
                "stripe" => Some(EntitlementSource::Stripe),
                "admin" => Some(EntitlementSource::Admin),
                _ => None,
            });
        Some(Self {
            tier,
            source,
            started_at: text("started_at"),
            expires_at: text("expires_at"),
        })
    }
}

/// One award (`user_badges` row, or `loud` while the tier is Loud).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserBadge {
    pub id: String,
    /// RFC 3339, or empty when the server sent none.
    #[serde(default)]
    pub awarded_at: String,
}

impl UserBadge {
    /// One item of the list: an object with a string `id`; a missing or
    /// non-string `awarded_at` is empty.
    pub fn parse(value: &Value) -> Option<Self> {
        let raw = value.as_object()?;
        let id = raw.get("id")?.as_str()?.to_owned();
        let awarded_at = raw
            .get("awarded_at")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        Some(Self { id, awarded_at })
    }
}

/// `entitlement`: absent, null or unreadable is `None` (free).
pub(crate) fn lenient_entitlement<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Box<Entitlement>>, D::Error> {
    let value = Value::deserialize(deserializer)?;
    Ok(Entitlement::parse(&value).map(Box::new))
}

/// `badges`: a list keeps the items that decode; anything else is `None`.
pub(crate) fn lenient_badges<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<UserBadge>>, D::Error> {
    Ok(match Value::deserialize(deserializer)? {
        Value::Array(items) => Some(items.iter().filter_map(UserBadge::parse).collect()),
        _ => None,
    })
}
