//! Decoding of the DM, presence, member and profile bodies.

use serde_json::json;
use yap_protocol::payload::{PresenceList, PresenceUpdateEvent, UserProfileUpdate};
use yap_protocol::rest::{DmChannel, Member, User};

#[test]
fn presence_update_without_custom_status_should_leave_it_unchanged() {
    let event: PresenceUpdateEvent =
        serde_json::from_value(json!({ "user_id": "u", "status": "idle" })).expect("decodes");
    assert_eq!(event.custom_status, None);
}

#[test]
fn presence_update_with_null_custom_status_should_clear_it() {
    let event: PresenceUpdateEvent = serde_json::from_value(
        json!({ "user_id": "u", "status": "online", "custom_status": null }),
    )
    .expect("decodes");
    assert_eq!(event.custom_status, Some(None));
}

#[test]
fn legacy_presence_list_should_carry_only_ids() {
    let list: PresenceList =
        serde_json::from_value(json!({ "user_ids": ["a", "b"] })).expect("decodes");
    assert_eq!((list.users, list.user_ids.len()), (None, 2));
}

#[test]
fn member_roles_should_skip_a_role_that_does_not_decode() {
    let member: Member = serde_json::from_value(json!({
        "user": { "id": "u", "username": "u" },
        "roles": [{ "id": "r", "name": "Mods", "position": 3, "is_default": 0 }, { "name": 5 }],
    }))
    .expect("decodes");
    assert_eq!(member.roles.len(), 1);
}

#[test]
fn highest_role_should_take_the_first_of_equal_positions() {
    let member: Member = serde_json::from_value(json!({
        "user": { "id": "u" },
        "roles": [
            { "id": "a", "position": 2 }, { "id": "b", "position": 2 }, { "id": "c", "position": 1 },
        ],
    }))
    .expect("decodes");
    assert_eq!(member.highest_role().map(|r| r.id.as_str()), Some("a"));
}

#[test]
fn dm_channel_should_decode_the_recipient() {
    let dm: DmChannel = serde_json::from_value(json!({
        "id": "d", "channel_type": "dm", "name": "Ann", "created_at": null,
        "recipient": { "id": "u", "username": "ann", "status": "dnd", "custom_status": null },
    }))
    .expect("decodes");
    assert_eq!(dm.recipient.status.as_deref(), Some("dnd"));
}

#[test]
fn profile_update_should_change_only_the_fields_it_carries() {
    let update: UserProfileUpdate =
        serde_json::from_value(json!({ "user_id": "u", "display_name": null, "bio": "hi" }))
            .expect("decodes");
    let mut user = User {
        id: "u".into(),
        username: "ann".into(),
        display_name: Some("Ann".into()),
        avatar_url: Some("/a.png".into()),
        ..User::default()
    };
    update.apply(&mut user);
    assert_eq!(
        (user.display_name, user.avatar_url, user.bio),
        (None, Some("/a.png".into()), Some("hi".into()))
    );
}
