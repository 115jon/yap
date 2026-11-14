//! Decoding of the invite preview and the join answer.

use serde_json::json;
use yap_protocol::rest::{InvitePreview, JoinedInvite};

#[test]
fn invite_preview_should_decode_the_server_and_inviter() {
    let preview: InvitePreview = serde_json::from_value(json!({
        "code": "a1b2c3d4",
        "server": { "id": "srv", "name": "Lab", "icon_url": null, "member_count": 12 },
        "inviter": { "username": "jon", "display_name": null, "avatar_url": null },
    }))
    .expect("decodes");
    assert_eq!(
        (
            preview.server.name.as_str(),
            preview.server.member_count,
            preview.inviter.username.as_str()
        ),
        ("Lab", 12, "jon")
    );
}

#[test]
fn join_answer_should_decode_a_stored_row_with_integer_flags() {
    let joined: JoinedInvite = serde_json::from_value(json!({
        "joined": true,
        "server": { "id": "srv", "name": "Lab", "owner_id": "u", "icon_url": null,
                    "invites_paused": 0, "created_at": "2026-11-01T00:00:00.000Z" },
    }))
    .expect("decodes");
    assert_eq!(joined.server.map(|s| s.invites_paused), Some(Some(false)));
}

#[test]
fn join_answer_should_keep_going_without_a_server_row() {
    let joined: JoinedInvite =
        serde_json::from_value(json!({ "already_member": true, "server": null })).expect("decodes");
    assert!(joined.already_member && joined.server.is_none());
}
