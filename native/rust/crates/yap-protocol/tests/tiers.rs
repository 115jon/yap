//! `entitlement` and `badges` on users/me and profiles, read as the web's
//! `parseEntitlement` and `parseUserBadges` read them.

use serde_json::json;
use yap_protocol::rest::User;
use yap_protocol::tiers::{Entitlement, EntitlementSource, Tier, UserBadge};

fn user(extra: serde_json::Value) -> User {
    let mut body = json!({ "id": "u1", "username": "spike" });
    for (key, value) in extra.as_object().unwrap() {
        body[key] = value.clone();
    }
    serde_json::from_value(body).expect("a user decodes whatever its plan says")
}

#[test]
fn users_me_carries_the_full_plan_and_badges() {
    let me = user(json!({
        "entitlement": {
            "tier": "founder",
            "source": "grant",
            "started_at": "2026-11-09T00:00:00.000Z",
            "expires_at": null,
            "stripe_customer_id": "never sent, ignored anyway",
        },
        "badges": [
            { "id": "founder", "awarded_at": "2026-11-09T00:00:00.000Z" },
            { "id": "early-yapper", "awarded_at": "2026-11-09T00:00:00.000Z" },
        ],
    }));
    assert_eq!(
        me.entitlement.as_deref(),
        Some(&Entitlement {
            tier: Tier::Founder,
            source: Some(EntitlementSource::Grant),
            started_at: Some("2026-11-09T00:00:00.000Z".into()),
            expires_at: None,
        })
    );
    let ids: Vec<&str> = me
        .badges
        .as_deref()
        .unwrap()
        .iter()
        .map(|b| b.id.as_str())
        .collect();
    assert_eq!(ids, ["founder", "early-yapper"]);
}

#[test]
fn a_profile_carries_only_the_tier() {
    let other = user(json!({ "entitlement": { "tier": "loud" }, "badges": [] }));
    let plan = other.entitlement.as_deref().unwrap();
    assert_eq!(plan.tier, Tier::Loud);
    assert_eq!((plan.source, plan.started_at.as_deref()), (None, None));
    assert_eq!(other.badges.as_deref(), Some(&[][..]));
}

#[test]
fn anything_unreadable_is_free() {
    for entitlement in [
        json!(null),
        json!("founder"),
        json!({ "tier": "platinum" }),
        json!({ "tier": 3 }),
        json!({}),
    ] {
        let u = user(json!({ "entitlement": entitlement }));
        assert_eq!(u.entitlement, None, "{entitlement}");
    }
    assert_eq!(user(json!({})).entitlement, None);
    // An unknown source and empty dates are dropped, the tier kept.
    let odd = user(json!({ "entitlement": {
        "tier": "loud", "source": "paypal", "started_at": "", "expires_at": 5,
    } }));
    assert_eq!(
        odd.entitlement.as_deref(),
        Some(&Entitlement {
            tier: Tier::Loud,
            ..Entitlement::default()
        })
    );
}

#[test]
fn badges_skip_what_does_not_decode() {
    let u = user(json!({ "badges": [
        { "id": "founder", "awarded_at": "2026-11-09T00:00:00Z" },
        { "id": 7 },
        "loud",
        { "awarded_at": "x" },
        { "id": "night-owl", "awarded_at": null },
    ] }));
    assert_eq!(
        u.badges.unwrap(),
        [
            UserBadge {
                id: "founder".into(),
                awarded_at: "2026-11-09T00:00:00Z".into()
            },
            UserBadge {
                id: "night-owl".into(),
                awarded_at: String::new()
            },
        ]
    );
    assert_eq!(user(json!({ "badges": "lots" })).badges, None);
    assert_eq!(user(json!({})).badges, None);
}

#[test]
fn the_cached_user_round_trips() {
    let me = user(json!({
        "entitlement": { "tier": "founder", "source": "grant" },
        "badges": [{ "id": "founder", "awarded_at": "2026-11-09T00:00:00Z" }],
    }));
    let text = serde_json::to_string(&me).unwrap();
    let back: User = serde_json::from_str(&text).unwrap();
    assert_eq!(back, me);
    // Nothing is written for a user without them.
    let plain = serde_json::to_value(user(json!({}))).unwrap();
    assert!(plain.get("entitlement").is_none() && plain.get("badges").is_none());
}
