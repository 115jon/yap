//! Tiers and perks (`src/lib/entitlements.ts`): what a person's plan lets
//! them do, and the three plans as the Yap Loud page shows them.
//!
//! Feature code asks [`has_perk`] and never compares tiers, so a new tier,
//! a trial or an expiry changes the table here, not every call site. The
//! server enforces upload sizes from its own read; 1440p and 4K
//! (`HdStreams`) are hidden by the client only, as on the web.

use yap_protocol::rest::User;
use yap_protocol::tiers::{Entitlement, Tier};

use crate::limits::{self, Kind};
use crate::store::time::parse_rfc3339;

/// `PERKS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Perk {
    BigUploads,
    HdStreams,
    AllNameStyles,
    AnimatedProfile,
    ProfileThemes,
    FounderEffect,
    /// `customWidgets`: a Custom profile widget shows to others.
    CustomWidgets,
}

impl Perk {
    pub const ALL: [Self; 7] = [
        Self::BigUploads,
        Self::HdStreams,
        Self::AllNameStyles,
        Self::AnimatedProfile,
        Self::ProfileThemes,
        Self::FounderEffect,
        Self::CustomWidgets,
    ];
}

/// `TIER_PERKS`: Loud has everything but the Founder-only effect; Founder
/// has everything, forever.
fn tier_has(tier: Tier, perk: Perk) -> bool {
    match tier {
        Tier::Free => false,
        Tier::Loud => perk != Perk::FounderEffect,
        Tier::Founder => true,
    }
}

/// `effectiveTier`: the tier in force at `now_ms`. No entitlement is free,
/// and an expired plan (a lapsed subscription the webhook has not cleared
/// yet) counts as free. An expiry that does not parse is ignored, as
/// `Date.parse` giving NaN is.
pub fn effective_tier(entitlement: Option<&Entitlement>, now_ms: i64) -> Tier {
    let Some(entitlement) = entitlement else {
        return Tier::Free;
    };
    let expired = entitlement
        .expires_at
        .as_deref()
        .and_then(parse_rfc3339)
        .is_some_and(|at| at <= now_ms);
    if expired {
        Tier::Free
    } else {
        entitlement.tier
    }
}

/// `hasPerk`: whether `entitlement` includes `perk` at `now_ms`. The one
/// perk check.
pub fn has_perk(entitlement: Option<&Entitlement>, perk: Perk, now_ms: i64) -> bool {
    tier_has(effective_tier(entitlement, now_ms), perk)
}

/// A user's entitlement as the API sent it (none: free).
pub fn of(user: &User) -> Option<&Entitlement> {
    user.entitlement.as_deref()
}

/// The share qualities the signed-in user may pick
/// (`getAvailableStreamQualities`): 1440p and 4K only with `HdStreams`.
pub fn stream_qualities(entitlement: Option<&Entitlement>, now_ms: i64) -> Vec<&'static str> {
    const ALL: [&str; 8] = [
        "720p30", "720p60", "1080p30", "1080p60", "1440p30", "1440p60", "4k30", "4k60",
    ];
    let hd = has_perk(entitlement, Perk::HdStreams, now_ms);
    ALL.into_iter()
        .filter(|id| hd || !is_hd_stream_quality(id))
        .collect()
}

/// 1440p and 4K (`isHdStreamQuality`).
pub fn is_hd_stream_quality(id: &str) -> bool {
    id.starts_with("1440p") || id.starts_with("4k")
}

// Plans, for the tier cards

/// `TIER_NAMES`.
pub fn tier_name(tier: Tier) -> &'static str {
    match tier {
        Tier::Free => "Yap",
        Tier::Loud => "Yap Loud",
        Tier::Founder => "Founder",
    }
}

/// What a plan card lists: a line as written, or the uploads line, which
/// the card fills in from the enforced limit (`UPLOADS_FEATURE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    Uploads,
    Text(&'static str),
}

/// `TierPlan`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TierPlan {
    pub tier: Tier,
    pub name: &'static str,
    /// Under the name: the price, or how it is had.
    pub price: &'static str,
    pub blurb: &'static str,
    pub features: &'static [Feature],
}

/// `TIER_PLANS` (spec 2026-11-09). The Loud price is a placeholder until
/// billing ships.
pub const TIER_PLANS: [TierPlan; 3] = [
    TierPlan {
        tier: Tier::Free,
        name: "Yap",
        price: "Free",
        blurb: "Everything you need to hang out.",
        features: &[
            Feature::Uploads,
            Feature::Text("1080p60 streams"),
            Feature::Text("Classic name styles"),
            Feature::Text("400% volume, soundboard and Listen Together"),
        ],
    },
    TierPlan {
        tier: Tier::Loud,
        name: "Yap Loud",
        price: "$4.99 a month",
        blurb: "Turn it up.",
        features: &[
            Feature::Uploads,
            Feature::Text("1440p and 4K streams"),
            Feature::Text("Every name style and animated effect"),
            Feature::Text("Animated avatar, banner and nameplate"),
            Feature::Text("Custom profile themes"),
            Feature::Text("The Loud badge"),
        ],
    },
    TierPlan {
        tier: Tier::Founder,
        name: "Founder",
        price: "Lifetime",
        blurb: "For everyone who was here before launch.",
        features: &[
            Feature::Text("Everything in Loud, forever"),
            Feature::Text("The Founder badge"),
            Feature::Text("A Founder-only name effect and frame"),
            Feature::Text("Early access"),
        ],
    },
];

/// `featureText`: the uploads line carries the size that plan is held to
/// today (`clientUploadLimitFor({ tier })`), never more than the server
/// takes.
pub fn feature_text(feature: Feature, tier: Tier) -> String {
    match feature {
        Feature::Text(text) => text.to_owned(),
        Feature::Uploads => {
            let plan = Entitlement {
                tier,
                ..Entitlement::default()
            };
            let limit = limits::client_upload_limit_for(Some(&plan), Kind::Attachment, 0);
            format!("{} uploads", limits::format_limit(limit))
        }
    }
}

/// LoudPanel.tsx `PLAN_DESCRIPTION`.
pub fn plan_description(tier: Tier) -> &'static str {
    match tier {
        Tier::Founder => "Here before launch: everything in Yap Loud, for life.",
        Tier::Loud => "Bigger uploads, HD streams and every name style.",
        Tier::Free => "The free plan. Yap Loud adds bigger uploads and HD streams.",
    }
}

/// The plan row's value: "Founder · lifetime", else the tier's name.
pub fn plan_label(tier: Tier) -> &'static str {
    match tier {
        Tier::Founder => "Founder · lifetime",
        other => tier_name(other),
    }
}

/// TierCards.tsx `currentLine`: the status line on the card of the plan in
/// force. `renews` is the expiry as `Nov 5, 2026`, when there is one.
pub fn current_line(tier: Tier, renews: Option<&str>) -> String {
    match (tier, renews) {
        (Tier::Founder, _) => "Founder · lifetime".to_owned(),
        (Tier::Loud, Some(date)) => format!("Your plan · renews {date}"),
        _ => "Your plan".to_owned(),
    }
}

/// TierCards.tsx `PlanAction`: what a card offers. Founders have nothing to
/// buy or manage; free sees Upgrade on the Loud card, Loud sees Manage
/// there. Either is disabled ("Coming soon") until billing is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanAction {
    None,
    Upgrade,
    Manage,
}

impl PlanAction {
    pub fn key(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Upgrade => "upgrade",
            Self::Manage => "manage",
        }
    }
}

pub fn plan_action(plan: Tier, tier: Tier) -> PlanAction {
    match (plan, tier) {
        (_, Tier::Founder) => PlanAction::None,
        (Tier::Loud, Tier::Free) => PlanAction::Upgrade,
        (Tier::Loud, Tier::Loud) => PlanAction::Manage,
        _ => PlanAction::None,
    }
}

/// One card's top line: the current plan's status, "Included" (a founder's
/// view of the plans they have), "Before launch" (the Founder card), else
/// the price.
pub fn card_kicker(plan: &TierPlan, tier: Tier, renews: Option<&str>) -> String {
    if plan.tier == tier {
        current_line(tier, renews)
    } else if tier == Tier::Founder {
        "Included".to_owned()
    } else if plan.tier == Tier::Founder {
        "Before launch".to_owned()
    } else {
        plan.price.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_793_000_000_000; // 2026-10-26

    fn plan(tier: Tier, expires_at: Option<&str>) -> Entitlement {
        Entitlement {
            tier,
            expires_at: expires_at.map(str::to_owned),
            ..Entitlement::default()
        }
    }

    #[test]
    fn perks_follow_the_tier_table() {
        for perk in Perk::ALL {
            assert!(!has_perk(None, perk, NOW), "{perk:?}");
            assert!(!has_perk(Some(&plan(Tier::Free, None)), perk, NOW));
            assert!(has_perk(Some(&plan(Tier::Founder, None)), perk, NOW));
            assert_eq!(
                has_perk(Some(&plan(Tier::Loud, None)), perk, NOW),
                perk != Perk::FounderEffect
            );
        }
    }

    #[test]
    fn an_expired_plan_reads_as_free() {
        let lapsed = plan(Tier::Loud, Some("2026-01-01T00:00:00.000Z"));
        assert_eq!(effective_tier(Some(&lapsed), NOW), Tier::Free);
        assert!(!has_perk(Some(&lapsed), Perk::BigUploads, NOW));
        let live = plan(Tier::Loud, Some("2027-01-01T00:00:00Z"));
        assert_eq!(effective_tier(Some(&live), NOW), Tier::Loud);
        // Exactly at the expiry is expired (`expiresAt <= now`).
        let at = parse_rfc3339("2026-12-01T00:00:00Z").unwrap();
        let edge = plan(Tier::Loud, Some("2026-12-01T00:00:00Z"));
        assert_eq!(effective_tier(Some(&edge), at), Tier::Free);
        assert_eq!(effective_tier(Some(&edge), at - 1), Tier::Loud);
        // An expiry that does not parse is ignored.
        let odd = plan(Tier::Founder, Some("whenever"));
        assert_eq!(effective_tier(Some(&odd), NOW), Tier::Founder);
    }

    #[test]
    fn hd_qualities_need_the_perk() {
        assert_eq!(
            stream_qualities(None, NOW),
            ["720p30", "720p60", "1080p30", "1080p60"]
        );
        assert_eq!(
            stream_qualities(Some(&plan(Tier::Founder, None)), NOW).len(),
            8
        );
        assert!(is_hd_stream_quality("4k60"));
        assert!(is_hd_stream_quality("1440p30"));
        assert!(!is_hd_stream_quality("1080p60"));
    }

    #[test]
    fn plan_cards_match_the_web() {
        assert_eq!(feature_text(Feature::Uploads, Tier::Free), "25 MB uploads");
        assert_eq!(feature_text(Feature::Uploads, Tier::Loud), "500 MB uploads");
        assert_eq!(plan_label(Tier::Founder), "Founder · lifetime");
        assert_eq!(plan_label(Tier::Loud), "Yap Loud");
        assert_eq!(plan_label(Tier::Free), "Yap");
        assert_eq!(
            current_line(Tier::Loud, Some("Dec 1, 2026")),
            "Your plan · renews Dec 1, 2026"
        );
        assert_eq!(current_line(Tier::Loud, None), "Your plan");
        assert_eq!(plan_action(Tier::Loud, Tier::Free), PlanAction::Upgrade);
        assert_eq!(plan_action(Tier::Loud, Tier::Loud), PlanAction::Manage);
        assert_eq!(plan_action(Tier::Loud, Tier::Founder), PlanAction::None);
        assert_eq!(plan_action(Tier::Founder, Tier::Free), PlanAction::None);
        assert_eq!(plan_action(Tier::Free, Tier::Free), PlanAction::None);
        let [free, loud, founder] = &TIER_PLANS;
        assert_eq!(card_kicker(free, Tier::Founder, None), "Included");
        assert_eq!(
            card_kicker(founder, Tier::Founder, None),
            "Founder · lifetime"
        );
        assert_eq!(card_kicker(founder, Tier::Free, None), "Before launch");
        assert_eq!(card_kicker(loud, Tier::Free, None), "$4.99 a month");
        assert_eq!(card_kicker(free, Tier::Free, None), "Your plan");
    }
}
