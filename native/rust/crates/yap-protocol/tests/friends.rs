//! Decoding of the `/api/friends` bodies and the relationship dispatches.

use serde_json::json;
use yap_protocol::payload::RelationshipRemove;
use yap_protocol::rest::{FriendRequestSent, Relationship};

#[test]
fn a_listed_relationship_should_carry_its_type_and_user() {
    let rel: Relationship = serde_json::from_value(json!({
        "user": { "id": "u1", "username": "alice", "display_name": null,
                  "avatar_url": null, "avatar_display": null, "status": "online",
                  "custom_status": "hi" },
        "type": 2,
        "created_at": "2026-11-01T10:00:00.000Z",
    }))
    .expect("decodes");
    assert_eq!(
        (rel.kind, rel.user.username.as_str()),
        (Relationship::INCOMING, "alice")
    );
}

#[test]
fn a_sent_request_should_report_its_type() {
    let sent: FriendRequestSent =
        serde_json::from_value(json!({ "user": { "id": "u1" }, "type": 0 })).expect("decodes");
    assert_eq!(sent.kind, Some(0));
}

#[test]
fn a_relationship_remove_should_name_the_user() {
    let remove: RelationshipRemove =
        serde_json::from_value(json!({ "user_id": "u1" })).expect("decodes");
    assert_eq!(remove.user_id, "u1");
}
