//! The collectibles catalog as production serves it (a trimmed copy of
//! `GET /api/collectibles/catalog`: one collection, one item per kind and
//! two colour variants).

use yap_protocol::collectibles::{ApplyResponse, CatalogResponse};

fn catalog() -> CatalogResponse {
    serde_json::from_str(include_str!("fixtures/collectibles-catalog.json")).expect("catalog")
}

#[test]
fn every_item_should_decode() {
    assert_eq!(catalog().catalog.items.len(), 6);
}

#[test]
fn collections_should_list_their_items() {
    assert_eq!(catalog().catalog.categories[0].item_ids.len(), 6);
}

#[test]
fn effects_should_carry_their_layers() {
    let layers = catalog()
        .catalog
        .items
        .iter()
        .find(|i| i.kind == "profile_effect")
        .and_then(|i| i.profile_effect.as_ref())
        .map(|e| e.effects.len());
    assert_eq!(layers, Some(2));
}

#[test]
fn frames_should_carry_their_geometry() {
    let width = catalog()
        .catalog
        .items
        .iter()
        .find(|i| i.kind == "profile_frame")
        .and_then(|i| i.frame.as_ref())
        .and_then(|f| f.inner_width);
    assert_eq!(width, Some(1200.0));
}

#[test]
fn a_broken_item_should_be_skipped() {
    let body = r#"{"catalog":{"categories":[],"items":[{"id":1},{"id":"a:1","skuId":"1","kind":"nameplate"}]}}"#;
    let parsed: CatalogResponse = serde_json::from_str(body).expect("catalog");
    assert_eq!(parsed.catalog.items.len(), 1);
}

#[test]
fn an_apply_answer_should_carry_the_nameplate() {
    let body = r#"{"ok":true,"item":null,"items":[],"user":{"avatar_display":{"version":1},"nameplate_url":"https://cdn.discordapp.com/x.webm","nameplate_content_type":"video/webm","updated_at":"2026-11-05T00:00:00Z"}}"#;
    let parsed: ApplyResponse = serde_json::from_str(body).expect("apply");
    assert_eq!(
        parsed.user.nameplate_content_type.as_deref(),
        Some("video/webm")
    );
}
