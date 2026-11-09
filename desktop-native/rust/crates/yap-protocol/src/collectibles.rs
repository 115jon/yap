//! The collectibles shop's wire types: `GET /api/collectibles/catalog`
//! (src/lib/collectibles-catalog.ts) and `PATCH /api/collectibles/apply`
//! (src/routes/api/collectibles/apply.ts). Every field is optional on the
//! wire; a malformed item or collection is skipped, not fatal.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::rest::lenient_vec;

/// `GET /api/collectibles/catalog` → `{ catalog }`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct CatalogResponse {
    #[serde(default)]
    pub catalog: Catalog,
}

/// The whole shop: collections in the source shop's order, then items.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    #[serde(default, deserialize_with = "lenient_vec")]
    pub categories: Vec<Category>,
    #[serde(default, deserialize_with = "lenient_vec")]
    pub items: Vec<Item>,
    /// ISO time of the last upstream sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synced_at: Option<String>,
    /// The upstream is unreachable and this copy may be old.
    #[serde(default)]
    pub stale: bool,
}

/// One collection (`CollectibleCatalogCategory`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hero_banner_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub featured_block_url: Option<String>,
    /// A CSS background, usually `linear-gradient(180deg, #a 0%, #b 100%)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marketing_title: Option<String>,
    /// When the source shop pulls the collection (ISO time).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unpublished_at: Option<String>,
    #[serde(default)]
    pub item_ids: Vec<String>,
}

/// The source shop's price.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct Price {
    #[serde(default)]
    pub amount: i64,
    #[serde(default)]
    pub exponent: u32,
    #[serde(default)]
    pub currency: String,
}

/// One profile effect layer, in the effect's 450 x 880 stage.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectLayer {
    #[serde(default)]
    pub src: String,
    #[serde(rename = "loop", default, skip_serializing_if = "Option::is_none")]
    pub looping: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_delay: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z_index: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub randomized_sources: Vec<Source>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize, Serialize)]
pub struct Position {
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct Source {
    #[serde(default)]
    pub src: String,
}

/// A profile effect's art.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileEffect {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation_type: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_preview_src: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reduced_motion_src: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub static_frame_src: Option<String>,
    #[serde(default, deserialize_with = "lenient_vec")]
    pub effects: Vec<EffectLayer>,
}

/// One profile frame layer (lib/profile-frame.ts).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct FrameLayer {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub src: String,
    /// `staple`, `rail` or `border`.
    #[serde(rename = "type", default)]
    pub kind: String,
    /// `front` or `back`.
    #[serde(default)]
    pub order: String,
    /// `top` or `bottom` (staples).
    #[serde(default)]
    pub anchor: String,
    #[serde(default)]
    pub responsive: bool,
}

/// A profile frame's geometry and art, in its authoring units.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overflow_top: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overflow_bottom: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overflow_horizontal: Option<f64>,
    #[serde(default, deserialize_with = "lenient_vec")]
    pub layers: Vec<FrameLayer>,
}

/// One item (`CollectibleCatalogItem`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    /// `<kind>:<skuId>`.
    pub id: String,
    pub sku_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// `avatar_decoration`, `profile_effect`, `nameplate` or `profile_frame`.
    pub kind: String,
    #[serde(default)]
    pub category_id: String,
    #[serde(default)]
    pub category_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price: Option<Price>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nitro_price: Option<Price>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_url: Option<String>,
    /// Decorations: the preset hash; nameplates: the asset folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub static_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animated_url: Option<String>,
    /// Nameplates: the colour family (`cobalt`, `crimson`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette: Option<String>,
    /// Colour variants share their base design's sku here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_of: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_effect: Option<ProfileEffect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<Frame>,
}

/// `PATCH /api/collectibles/apply`'s saved user fields.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct AppliedUser {
    #[serde(default)]
    pub avatar_display: Option<Value>,
    #[serde(default)]
    pub nameplate_url: Option<String>,
    #[serde(default)]
    pub nameplate_content_type: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// `PATCH /api/collectibles/apply` → `{ ok, user }`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct ApplyResponse {
    #[serde(default)]
    pub user: AppliedUser,
}
