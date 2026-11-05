//! Attachments, reactions, embeds and GIF picker bodies as the server sends them.

use serde_json::json;
use yap_protocol::payload::ReactionEvent;
use yap_protocol::rest::{GifPage, Message, MessagePatch, UploadedFile};

fn message(extra: serde_json::Value) -> Message {
    let mut body = json!({ "id": "m1", "channel_id": "c1", "content": "hi" });
    if let (Some(body), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
        body.extend(extra.clone());
    }
    serde_json::from_value(body).expect("message decodes")
}

#[test]
fn reactions_should_decode_with_their_users() {
    let m = message(json!({ "reactions": [
        { "emoji": "👍", "count": 2, "me": true, "users": ["a", "b"] }
    ]}));
    assert_eq!(m.reactions[0].users, ["a", "b"]);
}

#[test]
fn an_odd_reaction_should_be_skipped_not_fail_the_message() {
    let m = message(json!({ "reactions": [{ "count": 1 }, { "emoji": "❤️" }] }));
    assert_eq!(m.reactions.len(), 1);
}

#[test]
fn a_link_embed_should_keep_what_the_card_draws() {
    let m = message(json!({ "embeds": [{
        "id": "e1", "url": "https://github.com/KDAB/cxx-qt", "type": "link",
        "rawTitle": "cxx-qt", "provider": { "name": "GitHub" },
        "thumbnail": { "url": "https://x/y.png", "width": 1200, "height": 600 },
        "imageLayout": "large", "fields": []
    }]}));
    let embed = &m.embeds[0];
    assert_eq!(
        (
            embed.raw_title.as_deref(),
            embed.image_layout.as_deref(),
            embed.thumbnail.as_ref().and_then(|t| t.width)
        ),
        (Some("cxx-qt"), Some("large"), Some(1200.0))
    );
}

#[test]
fn an_embed_with_a_bad_part_should_keep_the_rest() {
    let m = message(json!({ "embeds": [{ "url": "https://a", "author": 5, "rawTitle": "t" }] }));
    assert_eq!(m.embeds[0].raw_title.as_deref(), Some("t"));
}

#[test]
fn attachments_should_read_nsfw_as_bool_or_int() {
    let m = message(json!({ "attachments": [
        { "id": "a", "filename": "x.png", "is_nsfw": 1 },
        { "id": "b", "filename": "y.png", "is_nsfw": false }
    ]}));
    assert_eq!(
        m.attachments.iter().map(|a| a.is_nsfw).collect::<Vec<_>>(),
        [true, false]
    );
}

#[test]
fn a_patch_with_empty_embeds_should_clear_them() {
    let patch: MessagePatch =
        serde_json::from_value(json!({ "id": "m1", "embeds": [] })).expect("patch");
    assert_eq!(patch.embeds, Some(Vec::new()));
}

#[test]
fn a_patch_without_embeds_should_leave_them() {
    let patch: MessagePatch =
        serde_json::from_value(json!({ "id": "m1", "content": "x" })).expect("patch");
    assert_eq!(patch.embeds, None);
}

#[test]
fn reaction_events_should_decode() {
    let event: ReactionEvent = serde_json::from_value(json!({
        "message_id": "m1", "channel_id": "c1", "user_id": "u1", "emoji": "🔥"
    }))
    .expect("event");
    assert_eq!(event.emoji, "🔥");
}

#[test]
fn an_upload_answer_should_decode() {
    let file: UploadedFile = serde_json::from_value(json!({
        "id": "att1", "file_url": "/api/attachments/c/att1/a b.png",
        "file_name": "a b.png", "file_size": 12, "content_type": "image/png"
    }))
    .expect("upload");
    assert_eq!(file.id, "att1");
}

#[test]
fn a_gif_page_should_decode_its_items_and_cursor() {
    let page: GifPage = serde_json::from_value(json!({
        "results": [{
            "id": "g1", "title": "Pengu", "provider": "klipy",
            "preview": { "url": "https://static.klipy.com/a.mp4", "width": 300, "height": 300,
                         "sizeBytes": 16937, "contentType": "video/mp4" },
            "send": { "url": "https://static.klipy.com/a.gif", "width": 300, "height": 300,
                      "sizeBytes": 68275, "contentType": "image/gif" },
            "mediaType": "gifs"
        }],
        "next": "Mg=="
    }))
    .expect("page");
    assert_eq!(
        (page.results[0].send.size_bytes, page.next.as_deref()),
        (68275, Some("Mg=="))
    );
}
