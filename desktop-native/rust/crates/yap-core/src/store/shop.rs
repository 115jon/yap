//! The collectibles shop (ShopView.tsx, ShopItemCard.tsx, ShopItemDetail.tsx)
//! and the profile editor's pickers (CollectiblePicker.tsx): what the UI
//! asks for ([`ShopCommand`]), the two REST calls, what the store keeps
//! ([`ShopState`]) and what QML draws ([`ShopView`]).
//!
//! Everything is free: an equip is `PATCH /api/collectibles/apply` with the
//! slot and the sku. The server checks the profile's `updated_at` itself;
//! a 409 (it changed under us) is retried once, then shown.
//!
//! The page is a virtualised list, so its body comes out as rows: a hero,
//! headings, horizontal rails and grid rows of `columns` cards (QML reports
//! how many fit).

use serde_json::{json, Value};
use yap_protocol::collectibles::{
    AppliedUser, ApplyResponse, Catalog, CatalogResponse, Category, Item,
};
use yap_protocol::rest::User;

use super::time::parse_rfc3339;
use super::{Method, RequestId};
use crate::api::ApiError;
use crate::collectibles::{self as c, Family, Kind, KINDS};

/// The shop's tab: Featured, or one kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Featured,
    Kind(Kind),
}

impl Tab {
    pub fn parse(text: &str) -> Self {
        Kind::parse(text).map_or(Self::Featured, Self::Kind)
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Featured => "featured",
            Self::Kind(kind) => kind.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShopCommand {
    /// The shop page or a picker opened: load the catalog (again after 30
    /// minutes). `now_ms` is the UI's clock (collection status, freshness).
    Open {
        now_ms: i64,
    },
    Tab(String),
    Query(String),
    /// A collection's page, or back to the shop.
    Collection(Option<String>),
    /// How many cards fit a grid row.
    Columns(u32),
    /// An item's detail (by the family's base item id), or closed.
    Detail(Option<String>),
    /// A colour variant in the open detail.
    Variant(String),
    /// Equip an item (by id).
    Equip(String),
    /// Empty a slot (`avatar_decoration`, ...).
    Remove(String),
    /// The profile editor's picker for a kind, or closed.
    Picker(Option<String>),
    PickerQuery(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShopRequest {
    Catalog,
    /// The body as the web sends it: `{kind, skuId, avatarDisplay}`.
    Apply(Value),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShopResponse {
    Catalog(Box<Catalog>),
    Applied(Box<AppliedUser>),
}

impl ShopRequest {
    pub fn method(&self) -> Method {
        match self {
            Self::Catalog => Method::Get,
            Self::Apply(_) => Method::Patch,
        }
    }

    pub fn path(&self) -> String {
        match self {
            Self::Catalog => "/api/collectibles/catalog".into(),
            Self::Apply(_) => "/api/collectibles/apply".into(),
        }
    }

    pub fn body(&self) -> Value {
        match self {
            Self::Catalog => Value::Null,
            Self::Apply(body) => body.clone(),
        }
    }

    pub fn decode(&self, body: Value) -> Result<ShopResponse, ApiError> {
        match self {
            Self::Catalog => serde_json::from_value::<CatalogResponse>(body)
                .map(|r| ShopResponse::Catalog(Box::new(r.catalog)))
                .map_err(|e| ApiError::Decode(format!("catalog: {e}"))),
            Self::Apply(_) => serde_json::from_value::<ApplyResponse>(body)
                .map(|r| ShopResponse::Applied(Box::new(r.user)))
                .map_err(|e| ApiError::Decode(format!("apply: {e}"))),
        }
    }
}

/// What a request was for.
#[derive(Debug, Clone, PartialEq)]
pub enum ShopOp {
    Catalog,
    /// The body, and whether it is already the retry.
    Apply {
        body: Value,
        retried: bool,
    },
}

/// What the store keeps.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShopState {
    pub(super) catalog: Option<Box<Catalog>>,
    pub(super) loaded_ms: i64,
    pub(super) now_ms: i64,
    pub(super) loading: Option<RequestId>,
    pub(super) error: String,
    pub(super) tab: Tab,
    pub(super) query: String,
    pub(super) collection: Option<String>,
    pub(super) columns: u32,
    pub(super) detail: Option<String>,
    pub(super) variant: Option<String>,
    pub(super) applying: Option<RequestId>,
    pub(super) action_error: String,
    pub(super) picker: Option<Kind>,
    pub(super) picker_query: String,
}

/// What QML draws: the header and state, the page's rows, the picker's
/// rows and the open detail, each JSON.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShopView {
    pub state: String,
    pub rows: String,
    pub picker: String,
    pub detail: String,
}

impl ShopView {
    /// The view before the store has said anything (nothing loaded yet).
    pub fn initial() -> Self {
        shop_view(&ShopState::default(), None)
    }
}

const FEATURED_ROWS: usize = 6;
const RAIL_ITEMS: usize = 12;

/// `relativeAge` with `Intl.RelativeTimeFormat("en", { numeric: "auto" })`.
fn relative_age(iso: Option<&str>, now_ms: i64) -> Option<String> {
    let at = parse_rfc3339(iso?)?;
    let minutes = ((now_ms - at).max(0) as f64 / 60_000.0).round() as i64;
    let plural = |n: i64, unit: &str| {
        if n == 1 {
            format!("1 {unit} ago")
        } else {
            format!("{n} {unit}s ago")
        }
    };
    Some(if minutes < 60 {
        if minutes == 0 {
            "now".to_owned()
        } else {
            plural(minutes, "minute")
        }
    } else {
        let hours = (minutes as f64 / 60.0).round() as i64;
        if hours < 48 {
            plural(hours, "hour")
        } else {
            let days = (hours as f64 / 24.0).round() as i64;
            if days == 1 {
                "yesterday".to_owned()
            } else {
                plural(days, "day")
            }
        }
    })
}

fn freshness(catalog: &Catalog, now_ms: i64) -> String {
    let age = relative_age(catalog.synced_at.as_deref(), now_ms);
    match (catalog.stale, age) {
        (true, Some(age)) => format!("Catalog may be out of date · synced {age}"),
        (true, None) => "Catalog may be out of date".into(),
        (false, Some(age)) => format!("Synced {age}"),
        (false, None) => String::new(),
    }
}

/// The ids of the items the user wears, by kind.
fn equipped(me: Option<&User>) -> Vec<String> {
    let display = me.and_then(|m| m.avatar_display.as_ref());
    KINDS
        .iter()
        .filter_map(|kind| c::selected_sku(display, *kind))
        .collect()
}

fn category_of<'a>(catalog: &'a Catalog, id: &str) -> Option<&'a Category> {
    catalog.categories.iter().find(|c| c.id == id)
}

fn card(family: &Family<'_>, catalog: &Catalog, worn: &[String], show_kind: bool) -> Value {
    let item = family.base;
    let kind = Kind::parse(&item.kind).unwrap_or(Kind::Decoration);
    let is_worn = family.variants.iter().any(|v| worn.contains(&v.sku_id));
    let dots: Vec<String> = family
        .variants
        .iter()
        .take(4)
        .map(|v| v.variant_color.clone().unwrap_or_default())
        .collect();
    let (still, motion) = item_art(item, kind);
    json!({
        "key": family.key,
        "id": item.id,
        "kind": kind.as_str(),
        "name": c::base_name(item),
        "kindLabel": kind.one(),
        "showKind": show_kind,
        "price": c::format_price(item).unwrap_or_default(),
        "equipped": is_worn,
        "dots": if family.variants.len() > 1 { dots } else { Vec::new() },
        "variants": family.variants.len(),
        "bg": c::gradient(category_of(catalog, &item.category_id).and_then(|c| c.background.as_deref())),
        "still": still,
        "motion": motion,
        "frame": frame_of(item),
    })
}

/// A tile's still and its motion (decorations: APNG; nameplates: webm).
fn item_art(item: &Item, kind: Kind) -> (String, String) {
    match kind {
        Kind::Decoration => (
            c::decoration_still_url(item, 160),
            c::decoration_motion_url(item),
        ),
        Kind::Effect => (c::effect_still_url(item, 150.0), String::new()),
        Kind::Nameplate => (
            item.static_url
                .clone()
                .or_else(|| item.preview_url.clone())
                .unwrap_or_default(),
            item.animated_url.clone().unwrap_or_default(),
        ),
        Kind::Frame => (item.preview_url.clone().unwrap_or_default(), String::new()),
    }
}

fn frame_of(item: &Item) -> Value {
    c::selection_value(item)
        .filter(|_| item.kind == "profile_frame")
        .map(|value| {
            c::frame_json(Some(
                &json!({"version": 1, "collectibles": {"profileFrame": value}}),
            ))
        })
        .unwrap_or(Value::Null)
}

fn hero(category: &Category, count: usize, browse: bool, now_ms: i64) -> Value {
    let art = c::collection_art(category, 1600.0, true);
    let logo = category
        .logo_url
        .as_deref()
        .map(|l| c::resized_shop_image_url(l, 542.0, 136.0))
        .unwrap_or_default();
    let status = c::collection_status(category, now_ms);
    json!({
        "type": "hero",
        "id": category.id,
        "name": category.name,
        "count": count,
        "art": art.as_ref().map(|a| a.0.clone()).unwrap_or_default(),
        "logo": logo,
        "bg": c::gradient(category.background.as_deref()),
        "text": category.marketing_title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| category.summary.clone()),
        "status": status.as_ref().map(|s| s.1.clone()).unwrap_or_default(),
        "tone": status.map(|s| s.0).unwrap_or_default(),
        "browse": browse,
    })
}

fn heading(title: &str, count: Option<usize>, action: &str, collection: &str) -> Value {
    json!({
        "type": "heading",
        "title": title,
        "count": count.map_or(-1, |n| i64::try_from(n).unwrap_or(i64::MAX)),
        "action": action,
        "collection": collection,
    })
}

fn grid_rows(cards: Vec<Value>, columns: usize, out: &mut Vec<Value>) {
    for chunk in cards.chunks(columns.max(1)) {
        out.push(json!({ "type": "grid", "cards": chunk }));
    }
}

fn section_grids(
    sections: &[c::Section<'_>],
    catalog: &Catalog,
    worn: &[String],
    columns: usize,
    show_kind: bool,
    action: &str,
    out: &mut Vec<Value>,
) {
    for section in sections {
        let families = c::families(&section.items);
        out.push(heading(
            &section.category.name,
            Some(families.len()),
            if section.category.id == "__more" {
                ""
            } else {
                action
            },
            &section.category.id,
        ));
        let cards = families
            .iter()
            .map(|f| card(f, catalog, worn, show_kind))
            .collect();
        grid_rows(cards, columns, out);
    }
}

fn page_rows(state: &ShopState, me: Option<&User>) -> Vec<Value> {
    if state.loading.is_some() && state.catalog.is_none() {
        return vec![json!({ "type": "loading" })];
    }
    let Some(catalog) = state.catalog.as_deref() else {
        return vec![json!({
            "type": "empty",
            "title": "The shop didn't load",
            "text": if state.error.is_empty() { "Check your connection, then open the shop again." } else { state.error.as_str() },
        })];
    };
    let worn = equipped(me);
    let columns = state.columns.max(2) as usize;
    let mut out = Vec::new();
    let all = c::sections(catalog, None, "");
    if let Some(section) = state
        .collection
        .as_deref()
        .and_then(|id| all.iter().find(|s| s.category.id == id))
    {
        out.push(hero(
            section.category,
            section.items.len(),
            false,
            state.now_ms,
        ));
        for kind in KINDS {
            let items: Vec<&Item> = section
                .items
                .iter()
                .copied()
                .filter(|i| i.kind == kind.as_str())
                .collect();
            if items.is_empty() {
                continue;
            }
            let families = c::families(&items);
            out.push(heading(kind.many(), Some(families.len()), "", ""));
            let cards = families
                .iter()
                .map(|f| card(f, catalog, &worn, false))
                .collect();
            grid_rows(cards, columns, &mut out);
        }
        return out;
    }
    let kinds = match state.tab {
        Tab::Featured => None,
        Tab::Kind(kind) => Some(vec![kind]),
    };
    let query = state.query.trim();
    if !query.is_empty() {
        let sections = c::sections(catalog, kinds.as_deref(), query);
        let total: usize = sections.iter().map(|s| s.items.len()).sum();
        let text = if total == 0 {
            format!("Nothing matches “{query}”.")
        } else {
            format!(
                "{total} {} for “{query}”",
                if total == 1 { "result" } else { "results" }
            )
        };
        out.push(json!({ "type": "status", "text": text }));
        section_grids(
            &sections,
            catalog,
            &worn,
            columns,
            state.tab == Tab::Featured,
            "View collection",
            &mut out,
        );
        return out;
    }
    match state.tab {
        Tab::Featured => {
            if let Some(first) = all.first() {
                out.push(hero(first.category, first.items.len(), true, state.now_ms));
            }
            out.push(heading("Collections", Some(all.len()), "", ""));
            let collections: Vec<Value> = all
                .iter()
                .map(|s| {
                    let art = s
                        .category
                        .featured_block_url
                        .as_deref()
                        .map(|b| c::resized_shop_image_url(b, 520.0, 232.0))
                        .or_else(|| c::collection_art(s.category, 520.0, false).map(|a| a.0))
                        .unwrap_or_default();
                    let status = c::collection_status(s.category, state.now_ms);
                    json!({
                        "id": s.category.id,
                        "name": s.category.name,
                        "art": art,
                        "bg": c::gradient(s.category.background.as_deref()),
                        "status": status.as_ref().map(|s| s.1.clone()).unwrap_or_default(),
                        "tone": status.map(|s| s.0).unwrap_or_default(),
                    })
                })
                .collect();
            out.push(json!({ "type": "collections", "cards": collections }));
            for section in all.iter().take(FEATURED_ROWS) {
                out.push(heading(
                    &section.category.name,
                    None,
                    "See all",
                    &section.category.id,
                ));
                let cards: Vec<Value> = c::families(&section.items)
                    .iter()
                    .take(RAIL_ITEMS)
                    .map(|f| card(f, catalog, &worn, true))
                    .collect();
                out.push(json!({ "type": "rail", "cards": cards }));
            }
        }
        Tab::Kind(_) => {
            let sections = c::sections(catalog, kinds.as_deref(), "");
            section_grids(
                &sections,
                catalog,
                &worn,
                columns,
                false,
                "View collection",
                &mut out,
            );
        }
    }
    out
}

fn picker_rows(state: &ShopState, kind: Kind, me: Option<&User>) -> Vec<Value> {
    let Some(catalog) = state.catalog.as_deref() else {
        return vec![if state.loading.is_some() {
            json!({ "type": "loading" })
        } else {
            json!({ "type": "empty", "title": "The shop didn't load", "text": state.error })
        }];
    };
    let worn = equipped(me);
    let mut out = vec![json!({
        "type": "none",
        "label": kind.none_label(),
        "equipped": c::selected_sku(me.and_then(|m| m.avatar_display.as_ref()), kind).is_none(),
    })];
    let sections = c::sections(catalog, Some(&[kind]), &state.picker_query);
    // The picker is narrower than the page: three to a row.
    section_grids(&sections, catalog, &worn, 3, false, "", &mut out);
    out
}

/// The family an item belongs to (the same variant group).
fn family_of<'a>(catalog: &'a Catalog, id: &str) -> Option<(Kind, Vec<&'a Item>)> {
    let base = catalog.items.iter().find(|i| i.id == id)?;
    let kind = Kind::parse(&base.kind)?;
    let group = base
        .variant_of
        .as_deref()
        .unwrap_or(&base.sku_id)
        .to_owned();
    let members: Vec<&Item> = catalog
        .items
        .iter()
        .filter(|i| i.kind == base.kind && i.variant_of.as_deref().unwrap_or(&i.sku_id) == group)
        .collect();
    let families = c::families(&members);
    let family = families.into_iter().next()?;
    Some((kind, family.variants))
}

fn detail_json(state: &ShopState, me: Option<&User>) -> Value {
    let (Some(catalog), Some(id)) = (state.catalog.as_deref(), state.detail.as_deref()) else {
        return Value::Null;
    };
    let Some((kind, variants)) = family_of(catalog, id) else {
        return Value::Null;
    };
    let display = me.and_then(|m| m.avatar_display.as_ref());
    let worn = c::selected_sku(display, kind);
    let chosen = state
        .variant
        .as_deref()
        .and_then(|v| variants.iter().find(|i| i.id == v))
        .or_else(|| variants.iter().find(|i| Some(&i.sku_id) == worn.as_ref()))
        .or_else(|| variants.first())
        .copied();
    let Some(item) = chosen else {
        return Value::Null;
    };
    let category = category_of(catalog, &item.category_id);
    let status = category.and_then(|cat| c::collection_status(cat, state.now_ms));
    let preview = c::display_with(display, kind, Some(item));
    let me_id = me.map_or("", |m| m.id.as_str());
    let plate = if kind == Kind::Nameplate {
        let animated = item.animated_url.as_deref();
        let url = animated.or(item.static_url.as_deref());
        let content = if animated.is_some() {
            "video/webm"
        } else {
            "image/png"
        };
        serde_json::to_value(c::plate(Some(&preview), url, Some(content), me_id))
            .unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    let (still, motion) = item_art(item, kind);
    json!({
        "id": item.id,
        "baseId": id,
        "kind": kind.as_str(),
        "kindOne": kind.one(),
        "name": c::base_name(item),
        "collection": category.map(|c| c.name.as_str()).unwrap_or(item.category_name.as_str()),
        "bg": c::gradient(category.and_then(|c| c.background.as_deref())),
        "status": status.as_ref().map(|s| s.1.clone()).unwrap_or_default(),
        "tone": status.map(|s| s.0).unwrap_or_default(),
        "description": item.label.clone().or_else(|| item.summary.clone()).unwrap_or_default(),
        "variantName": c::variant_name(item).unwrap_or_else(|| "Base".into()),
        "variants": if variants.len() > 1 {
            variants.iter().map(|v| json!({
                "id": v.id,
                "label": c::variant_name(v).unwrap_or_else(|| v.name.clone()),
                "color": v.variant_color.clone().unwrap_or_default(),
                "selected": v.id == item.id,
            })).collect::<Vec<_>>()
        } else {
            Vec::new()
        },
        "price": c::format_price(item).unwrap_or_default(),
        "equipped": worn.as_deref() == Some(item.sku_id.as_str()),
        "equipLabel": format!("Equip {}", kind.one().to_lowercase()),
        "busy": state.applying.is_some(),
        "error": state.action_error,
        "still": still,
        "motion": motion,
        "deco": if kind == Kind::Decoration { c::decoration_motion_url(item) } else { c::decoration_url(display) },
        "effect": c::effect_json(Some(&preview)),
        "frame": c::frame_json(Some(&preview)),
        "plate": plate,
    })
}

fn counts(catalog: Option<&Catalog>) -> Value {
    let mut out = serde_json::Map::new();
    for kind in KINDS {
        let n = catalog.map_or(0, |c| {
            c.items.iter().filter(|i| i.kind == kind.as_str()).count()
        });
        out.insert(kind.as_str().into(), json!(n));
    }
    Value::Object(out)
}

/// Everything QML shows of the shop.
pub(super) fn shop_view(state: &ShopState, me: Option<&User>) -> ShopView {
    let catalog = state.catalog.as_deref();
    let collection = state
        .collection
        .as_deref()
        .and_then(|id| catalog.and_then(|c| category_of(c, id)))
        .map(|c| json!({ "id": c.id, "name": c.name }));
    let header = json!({
        "loaded": catalog.is_some(),
        "loading": state.loading.is_some(),
        "error": state.error,
        "freshness": catalog.map(|c| freshness(c, state.now_ms)).unwrap_or_default(),
        "counts": counts(catalog),
        "tab": state.tab.as_str(),
        "query": state.query,
        "collection": collection,
        "busy": state.applying.is_some(),
        "actionError": state.action_error,
        "picker": state.picker.map(Kind::as_str).unwrap_or_default(),
        "pickerTitle": state.picker.map(Kind::one).unwrap_or_default(),
        "pickerQuery": state.picker_query,
    });
    let rows = page_rows(state, me);
    let picker = state
        .picker
        .map(|k| picker_rows(state, k, me))
        .unwrap_or_default();
    ShopView {
        state: header.to_string(),
        rows: Value::Array(rows).to_string(),
        picker: Value::Array(picker).to_string(),
        detail: detail_json(state, me).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        let item = |kind: &str, sku: &str, name: &str| Item {
            id: format!("{kind}:{sku}"),
            sku_id: sku.into(),
            name: name.into(),
            kind: kind.into(),
            category_id: "c1".into(),
            asset: Some(format!("a_{sku}")),
            ..Item::default()
        };
        Catalog {
            categories: vec![Category {
                id: "c1".into(),
                name: "Summer".into(),
                item_ids: vec!["avatar_decoration:1".into(), "nameplate:2".into()],
                ..Category::default()
            }],
            items: vec![
                item("avatar_decoration", "1", "Duck"),
                item("nameplate", "2", "Wave"),
            ],
            ..Catalog::default()
        }
    }

    fn rows(state: &ShopState) -> Vec<Value> {
        serde_json::from_str(&shop_view(state, None).rows).unwrap_or_default()
    }

    #[test]
    fn featured_should_open_on_the_newest_collection() {
        let state = ShopState {
            catalog: Some(Box::new(catalog())),
            ..ShopState::default()
        };
        assert_eq!(rows(&state)[0]["type"], "hero");
    }

    #[test]
    fn a_kind_tab_should_grid_only_that_kind() {
        let state = ShopState {
            catalog: Some(Box::new(catalog())),
            tab: Tab::Kind(Kind::Nameplate),
            columns: 4,
            ..ShopState::default()
        };
        let r = rows(&state);
        assert_eq!(
            (r[1]["type"].as_str(), r[1]["cards"][0]["name"].as_str()),
            (Some("grid"), Some("Wave"))
        );
    }

    #[test]
    fn a_search_should_count_its_results() {
        let state = ShopState {
            catalog: Some(Box::new(catalog())),
            query: "duck".into(),
            ..ShopState::default()
        };
        assert_eq!(rows(&state)[0]["text"], "1 result for “duck”");
    }

    #[test]
    fn a_failed_load_should_say_so() {
        let state = ShopState {
            error: "Unable to load collectibles catalog".into(),
            ..ShopState::default()
        };
        assert_eq!(rows(&state)[0]["type"], "empty");
    }

    #[test]
    fn worn_items_should_read_equipped() {
        let me = User {
            id: "u".into(),
            avatar_display: Some(
                json!({"version": 1, "collectibles": {"nameplate": {"skuId": "2"}}}),
            ),
            ..User::default()
        };
        let state = ShopState {
            catalog: Some(Box::new(catalog())),
            tab: Tab::Kind(Kind::Nameplate),
            ..ShopState::default()
        };
        let r: Vec<Value> =
            serde_json::from_str(&shop_view(&state, Some(&me)).rows).unwrap_or_default();
        assert_eq!(r[1]["cards"][0]["equipped"], true);
    }

    #[test]
    fn the_detail_should_offer_to_equip_its_kind() {
        let state = ShopState {
            catalog: Some(Box::new(catalog())),
            detail: Some("avatar_decoration:1".into()),
            ..ShopState::default()
        };
        let d: Value = serde_json::from_str(&shop_view(&state, None).detail).unwrap_or_default();
        assert_eq!(d["equipLabel"], "Equip avatar decoration");
    }

    #[test]
    fn freshness_should_flag_a_stale_catalog() {
        let catalog = Catalog {
            stale: true,
            synced_at: Some("2026-10-24T00:00:00Z".into()),
            ..Catalog::default()
        };
        let now = parse_rfc3339("2026-11-05T00:00:00Z").unwrap_or_default();
        assert_eq!(
            freshness(&catalog, now),
            "Catalog may be out of date · synced 12 days ago"
        );
    }
}
