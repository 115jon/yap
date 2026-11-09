//! Badges (`src/lib/badges.ts`): the eight definitions in display order,
//! reading awards from the API, and each badge's tooltip lines. The cell
//! art is drawn by the dither provider (`cpp/ditherprovider.cpp`
//! `badge/<id>/...`, from `src/lib/badge-art.ts`).
//!
//! The server sends a profile's badges with `loud` already added while the
//! tier is Loud (`profileBadges`); the client keeps the known ids, one per
//! id, in their fixed order (`parseUserBadges`).

use serde_json::{json, Value};
use yap_protocol::tiers::UserBadge;

use crate::store::time::{civil_date, local_date, parse_rfc3339, LocalOffset};

/// One badge as the client shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadgeDef {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
}

const fn def(id: &'static str, name: &'static str, description: &'static str) -> BadgeDef {
    BadgeDef {
        id,
        name,
        description,
    }
}

/// `BADGES`, in display order (founder and loud first).
pub const BADGES: [BadgeDef; 8] = [
    def("founder", "Founder", "Here before launch. Lifetime Loud."),
    def("loud", "Loud", "Subscribed to Yap Loud."),
    def("yap-team", "Yap team", "Builds Yap."),
    def("bug-hunter", "Bug hunter", "Reported a confirmed bug."),
    def(
        "early-yapper",
        "Early yapper",
        "Joined in the first 100 users.",
    ),
    def(
        "server-owner",
        "Server owner",
        "Runs a server with 10+ members.",
    ),
    def("voice-regular", "Voice regular", "100 hours in voice."),
    def("night-owl", "Night owl", "Yapped at 3 a.m."),
];

/// `getBadge`.
pub fn get(id: &str) -> Option<&'static BadgeDef> {
    BADGES.iter().find(|badge| badge.id == id)
}

fn rank(id: &str) -> usize {
    BADGES.iter().position(|badge| badge.id == id).unwrap_or(99)
}

/// `parseUserBadges`: unknown ids dropped, one per id (the first kept),
/// sorted by the fixed rank.
pub fn known(badges: &[UserBadge]) -> Vec<UserBadge> {
    let mut out: Vec<UserBadge> = Vec::new();
    for badge in badges {
        if get(&badge.id).is_some() && !out.iter().any(|b| b.id == badge.id) {
            out.push(badge.clone());
        }
    }
    // Stable, so equal ranks keep their order (there are none).
    out.sort_by_key(|badge| rank(&badge.id));
    out
}

/// `monthsSince`: whole months from `started_at` to `now_ms` in UTC, at
/// least 1 (1 as well when the date does not parse).
pub fn months_since(started_at: &str, now_ms: i64) -> i64 {
    let Some(start) = parse_rfc3339(started_at) else {
        return 1;
    };
    let (sy, sm, sd) = civil_date(start.div_euclid(86_400_000));
    let (ey, em, ed) = civil_date(now_ms.div_euclid(86_400_000));
    let months = (ey - sy) * 12 + (em - sm) - i64::from(ed < sd);
    months.max(1)
}

/// `badgeTooltipLines`: the name, the description (Loud: "Subscribed for
/// 3 months."), and "Earned Nov 5, 2026" ("Since ..." for Loud) when the
/// award has a date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TooltipLines {
    pub name: String,
    pub description: String,
    pub earned: Option<String>,
}

pub fn tooltip_lines(badge: &UserBadge, now_ms: i64) -> TooltipLines {
    let def = get(&badge.id);
    let name = def.map_or(badge.id.as_str(), |d| d.name).to_owned();
    let mut description = def.map_or("", |d| d.description).to_owned();
    if badge.id == "loud" && !badge.awarded_at.is_empty() {
        let months = months_since(&badge.awarded_at, now_ms);
        let unit = if months == 1 { "month" } else { "months" };
        description = format!("Subscribed for {months} {unit}.");
    }
    // formatProfileReferenceDate: `Nov 5, 2026` in UTC.
    let earned = parse_rfc3339(&badge.awarded_at).map(|ms| {
        let date = local_date(ms, LocalOffset::UTC, false);
        let verb = if badge.id == "loud" {
            "Since"
        } else {
            "Earned"
        };
        format!("{verb} {date}")
    });
    TooltipLines {
        name,
        description,
        earned,
    }
}

/// The badges QML draws, as JSON `[{id, name, description, earned, label}]`
/// in display order; `label` is the key's accessible name ("Founder: Here
/// before launch. Lifetime Loud."). Empty (`""`) when there are none, so a
/// profile without badges shows nothing.
pub fn display_json(badges: Option<&[UserBadge]>, now_ms: i64) -> String {
    let shown = known(badges.unwrap_or_default());
    if shown.is_empty() {
        return String::new();
    }
    Value::Array(shown.iter().map(|b| display(b, now_ms)).collect()).to_string()
}

fn display(badge: &UserBadge, now_ms: i64) -> Value {
    let lines = tooltip_lines(badge, now_ms);
    json!({
        "id": badge.id,
        "name": lines.name,
        "description": lines.description,
        "earned": lines.earned.clone().unwrap_or_default(),
        "label": format!("{}: {}", lines.name, lines.description),
    })
}

/// The plan badges the Yap Loud page shows beside the plan (founder and
/// loud only), as [`display_json`].
pub fn plan_badges_json(badges: Option<&[UserBadge]>, now_ms: i64) -> String {
    let plan: Vec<UserBadge> = badges
        .unwrap_or_default()
        .iter()
        .filter(|b| b.id == "founder" || b.id == "loud")
        .cloned()
        .collect();
    display_json(Some(&plan), now_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn award(id: &str, at: &str) -> UserBadge {
        UserBadge {
            id: id.into(),
            awarded_at: at.into(),
        }
    }

    fn ms(s: &str) -> i64 {
        parse_rfc3339(s).unwrap()
    }

    #[test]
    fn awards_keep_known_ids_once_in_order() {
        let list = known(&[
            award("night-owl", "2026-11-01T03:00:00Z"),
            award("mystery", ""),
            award("founder", "2026-10-01T00:00:00Z"),
            award("founder", "2026-12-01T00:00:00Z"),
            award("yap-team", ""),
            award("loud", "2026-08-01T00:00:00Z"),
        ]);
        let ids: Vec<&str> = list.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, ["founder", "loud", "yap-team", "night-owl"]);
        // The first of a repeated id wins.
        assert_eq!(list[0].awarded_at, "2026-10-01T00:00:00Z");
    }

    #[test]
    fn months_count_whole_utc_months() {
        let now = ms("2026-11-09T12:00:00Z");
        assert_eq!(months_since("2026-08-09T00:00:00Z", now), 3);
        assert_eq!(months_since("2026-08-10T00:00:00Z", now), 2);
        assert_eq!(months_since("2026-11-01T00:00:00Z", now), 1);
        assert_eq!(months_since("2025-11-09T00:00:00Z", now), 12);
        assert_eq!(months_since("not a date", now), 1);
    }

    #[test]
    fn tooltips_match_the_web() {
        let now = ms("2026-11-09T12:00:00Z");
        let founder = tooltip_lines(&award("founder", "2026-11-05T08:00:00.000Z"), now);
        assert_eq!(founder.name, "Founder");
        assert_eq!(founder.description, "Here before launch. Lifetime Loud.");
        assert_eq!(founder.earned.as_deref(), Some("Earned Nov 5, 2026"));
        let loud = tooltip_lines(&award("loud", "2026-08-01T00:00:00Z"), now);
        assert_eq!(loud.description, "Subscribed for 3 months.");
        assert_eq!(loud.earned.as_deref(), Some("Since Aug 1, 2026"));
        let fresh = tooltip_lines(&award("loud", "2026-11-01T00:00:00Z"), now);
        assert_eq!(fresh.description, "Subscribed for 1 month.");
        let undated = tooltip_lines(&award("bug-hunter", ""), now);
        assert_eq!(undated.earned, None);
        assert_eq!(undated.description, "Reported a confirmed bug.");
    }

    #[test]
    fn display_json_is_empty_without_badges() {
        assert_eq!(display_json(None, 0), "");
        assert_eq!(display_json(Some(&[award("nope", "")]), 0), "");
        let shown: Value = serde_json::from_str(&display_json(
            Some(&[
                award("server-owner", ""),
                award("founder", "2026-11-05T00:00:00Z"),
            ]),
            ms("2026-11-09T00:00:00Z"),
        ))
        .unwrap();
        assert_eq!(shown[0]["id"], "founder");
        assert_eq!(
            shown[0]["label"],
            "Founder: Here before launch. Lifetime Loud."
        );
        assert_eq!(shown[0]["earned"], "Earned Nov 5, 2026");
        assert_eq!(shown[1]["id"], "server-owner");
        assert_eq!(shown[1]["earned"], "");
        let plan: Value = serde_json::from_str(&plan_badges_json(
            Some(&[award("yap-team", ""), award("founder", "")]),
            0,
        ))
        .unwrap();
        assert_eq!(plan.as_array().map(Vec::len), Some(1));
    }
}
