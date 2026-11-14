//! Reply counts on messages (thread chips) and the pin dispatches.

use serde_json::json;
use yap_protocol::payload::MessageUnpinEvent;
use yap_protocol::rest::{Message, ThreadSummary};

fn message(extra: serde_json::Value) -> Message {
    let mut value = json!({ "id": "m1", "channel_id": "c1", "content": "hi" });
    if let (Some(object), Some(more)) = (value.as_object_mut(), extra.as_object()) {
        object.extend(more.clone());
    }
    serde_json::from_value(value).expect("decodes")
}

#[test]
fn reply_count_should_decode_from_a_message() {
    assert_eq!(message(json!({ "reply_count": 3 })).reply_count, 3);
}

#[test]
fn a_missing_reply_count_should_be_zero() {
    assert_eq!(message(json!({})).reply_count, 0);
}

#[test]
fn a_null_reply_count_should_be_zero() {
    assert_eq!(message(json!({ "reply_count": null })).reply_count, 0);
}

#[test]
fn an_unpin_should_decode_its_id_and_channel() {
    let unpin: MessageUnpinEvent =
        serde_json::from_value(json!({ "id": "m1", "channel_id": "c1", "is_pinned": false }))
            .expect("decodes");
    assert_eq!(
        (unpin.id.as_str(), unpin.channel_id.as_deref()),
        ("m1", Some("c1"))
    );
}

#[test]
fn a_thread_summary_should_decode_the_threads_tab_row() {
    let summary: ThreadSummary = serde_json::from_value(json!({
        "id": "m1", "content": "root", "reply_count": 2,
        "last_reply_at": "2026-11-04T12:05:00.000Z", "created_at": "2026-11-04T12:00:00.000Z",
        "author": { "id": "u1", "username": "ada", "display_name": null, "avatar_url": null },
    }))
    .expect("decodes");
    assert_eq!((summary.author.label(), summary.reply_count), ("ada", 2));
}
