//! The collectibles shop's wire types: `GET /api/collectibles/catalog`
//! (src/lib/collectibles-catalog.ts) and `PATCH /api/collectibles/apply`
//! (src/routes/api/collectibles/apply.ts). Every field is optional on the
//! wire; a malformed item or collection is skipped, not fatal.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::rest::lenient_vec;

/// `GET /api/collectibles/catalog` → `{ catalog }`.
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export_to = "collectibles.ts", optional_fields = nullable))]
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct CatalogResponse {
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<Catalog>", optional))]
    pub catalog: Catalog,
}

/// The whole shop: collections in the source shop's order, then items.
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export_to = "collectibles.ts", optional_fields = nullable))]
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    #[serde(default, deserialize_with = "lenient_vec")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<Category>>", optional))]
    pub categories: Vec<Category>,
    #[serde(default, deserialize_with = "lenient_vec")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<Item>>", optional))]
    pub items: Vec<Item>,
    /// ISO time of the last upstream sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synced_at: Option<String>,
    /// The upstream is unreachable and this copy may be old.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<bool>", optional))]
    pub stale: bool,
}

/// One collection (`CollectibleCatalogCategory`).
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export_to = "collectibles.ts", optional_fields = nullable))]
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<String>", optional))]
    pub name: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<String>", optional))]
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
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<String>>", optional))]
    pub item_ids: Vec<String>,
}

/// The source shop's price.
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export_to = "collectibles.ts", optional_fields = nullable))]
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct Price {
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<i64>", optional))]
    pub amount: i64,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<u32>", optional))]
    pub exponent: u32,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<String>", optional))]
    pub currency: String,
}

/// One profile effect layer, in the effect's 450 x 880 stage.
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export_to = "collectibles.ts", optional_fields = nullable))]
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectLayer {
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<String>", optional))]
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
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<Source>>", optional))]
    pub randomized_sources: Vec<Source>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export_to = "collectibles.ts", optional_fields = nullable))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize, Serialize)]
pub struct Position {
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<f64>", optional))]
    pub x: f64,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<f64>", optional))]
    pub y: f64,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export_to = "collectibles.ts", optional_fields = nullable))]
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct Source {
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<String>", optional))]
    pub src: String,
}

/// A profile effect's art.
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export_to = "collectibles.ts", optional_fields = nullable))]
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
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<EffectLayer>>", optional))]
    pub effects: Vec<EffectLayer>,
}

/// One profile frame layer (lib/profile-frame.ts).
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export_to = "collectibles.ts", optional_fields = nullable))]
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct FrameLayer {
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<String>", optional))]
    pub id: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<String>", optional))]
    pub src: String,
    /// `staple`, `rail` or `border`.
    #[serde(rename = "type", default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<String>", optional))]
    pub kind: String,
    /// `front` or `back`.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<String>", optional))]
    pub order: String,
    /// `top` or `bottom` (staples).
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<String>", optional))]
    pub anchor: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<bool>", optional))]
    pub responsive: bool,
}

/// A profile frame's geometry and art, in its authoring units.
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export_to = "collectibles.ts", optional_fields = nullable))]
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
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<FrameLayer>>", optional))]
    pub layers: Vec<FrameLayer>,
}

/// One item (`CollectibleCatalogItem`).
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export_to = "collectibles.ts", optional_fields = nullable))]
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    /// `<kind>:<skuId>`.
    pub id: String,
    pub sku_id: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<String>", optional))]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// `avatar_decoration`, `profile_effect`, `nameplate` or `profile_frame`.
    pub kind: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<String>", optional))]
    pub category_id: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<String>", optional))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export_to = "collectibles.ts", optional_fields = nullable))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export_to = "collectibles.ts", optional_fields = nullable))]
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct ApplyResponse {
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<AppliedUser>", optional))]
    pub user: AppliedUser,
}
